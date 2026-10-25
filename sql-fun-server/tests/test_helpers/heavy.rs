use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Output, Stdio},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(15);

fn command() -> Command {
    let mut command = Command::new(
        std::env::var("SQL_FUN_TEST_SERVER_BIN").expect("SQL_FUN_TEST_SERVER_BIN must be set"),
    );
    command
        .env_remove("SQL_FUN_SERVER_IDLE_TIMEOUT")
        .env_remove("REQUEST_LENGTH_LIMIT")
        .env_remove("IDLE_TIMEOUT")
        .env_remove("AUTH_TIMEOUT")
        .env("USER", "idle-configuration-test");
    command
}

/// Starts one isolated server child per fixture to verify process termination,
/// PID cleanup, and real TCP lifetimes. No schema setup or process is shared.
/// Drop always kills/reaps a remaining child before deleting its /tmp namespace.
pub(crate) struct Server {
    child: Option<Child>,
    namespace: ::tempfile::TempDir,
    addr: SocketAddr,
    token: String,
}

impl Server {
    pub(crate) fn start(timeout: &str, read_timeout: &str) -> Self {
        Self::with_options(&["--server-idle-timeout", timeout], None, read_timeout)
    }

    pub(crate) fn with_options(
        options: &[&str],
        env_timeout: Option<&str>,
        read_timeout: &str,
    ) -> Self {
        let mut fixture = Self::new();
        fixture.launch(options, env_timeout, read_timeout);
        fixture
    }

    fn new() -> Self {
        Self {
            child: None,
            namespace: ::tempfile::Builder::new()
                .prefix("sql-fun-idle-")
                .tempdir_in("/tmp")
                .unwrap(),
            addr: "127.0.0.1:0".parse().unwrap(),
            token: String::new(),
        }
    }

    /// Runs argument parsing in a child with its own namespace and bounded
    /// lifetime, so a parser regression cannot leak a server or hang the suite.
    pub(crate) fn configuration_output(options: &[&str], env_timeout: Option<&str>) -> Output {
        let mut fixture = Self::new();
        let mut command = command();
        command
            .args(options)
            .arg("--user")
            .arg(fixture.namespace.path().file_name().unwrap())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(timeout) = env_timeout {
            command.env("SQL_FUN_SERVER_IDLE_TIMEOUT", timeout);
        }
        fixture.child = Some(command.spawn().unwrap());
        fixture.wait_for_exit();
        fixture.child.take().unwrap().wait_with_output().unwrap()
    }

    fn pid_path(&self) -> PathBuf {
        self.namespace.path().join("sql-fun-server.pid")
    }

    fn launch(&mut self, options: &[&str], env_timeout: Option<&str>, read_timeout: &str) {
        let mut command = command();
        command
            .args(options)
            .arg("--user")
            .arg(self.namespace.path().file_name().unwrap())
            .args(["--idle-timeout", read_timeout, "--auth-timeout", "5"])
            .stdin(Stdio::null())
            .stdout(Stdio::null());
        if let Some(timeout) = env_timeout {
            command.env("SQL_FUN_SERVER_IDLE_TIMEOUT", timeout);
        }
        self.child = Some(command.spawn().unwrap());
        let limit = Instant::now() + WAIT;
        loop {
            if let Ok(bytes) = std::fs::read(self.pid_path())
                && let Ok(registration) = ::serde_json::from_slice::<::serde_json::Value>(&bytes)
            {
                self.addr
                    .set_port(u16::try_from(registration["port"].as_u64().unwrap()).unwrap());
                registration["token"]
                    .as_str()
                    .unwrap()
                    .clone_into(&mut self.token);
                return;
            }
            assert!(
                self.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                "server exited before registration"
            );
            assert!(Instant::now() < limit, "server registration timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(crate) fn connect(&self) -> Connection {
        let stream = TcpStream::connect_timeout(&self.addr, WAIT).unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        stream.set_write_timeout(Some(WAIT)).unwrap();
        Connection {
            stream: BufReader::new(stream),
        }
    }

    pub(crate) fn authenticate(&self) -> Connection {
        let mut connection = self.connect();
        connection.authenticate(&self.token);
        connection
    }

    pub(crate) fn assert_running_for(&mut self, duration: Duration) {
        let limit = Instant::now() + duration;
        while Instant::now() < limit {
            assert!(
                self.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                "server exited while it should remain alive"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(crate) fn assert_idle_exit(&mut self) {
        let status = self.wait_for_exit();
        assert!(status.success(), "server failed: {status}");
        assert!(!self.pid_path().exists(), "owned PID file was not removed");
        assert!(
            TcpStream::connect_timeout(&self.addr, Duration::from_secs(1)).is_err(),
            "listener remains open"
        );
    }

    fn wait_for_exit(&mut self) -> ExitStatus {
        let limit = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < limit, "server did not exit");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(crate) fn restart(&mut self) {
        assert!(self.child.as_mut().unwrap().try_wait().unwrap().is_some());
        self.launch(&["--server-idle-timeout", "1"], None, "10");
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(crate) struct Connection {
    stream: BufReader<TcpStream>,
}

impl Connection {
    fn authenticate(&mut self, token: &str) {
        self.send(&::serde_json::json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {"sql_fun_token": token},
        }));
        let mut response = String::new();
        self.stream.read_line(&mut response).unwrap();
        assert_eq!(
            ::serde_json::from_str::<::serde_json::Value>(&response).unwrap(),
            ::serde_json::json!({"jsonrpc":"2.0","id":0,"result":"OK"})
        );
    }

    fn send(&mut self, request: &::serde_json::Value) {
        writeln!(self.stream.get_mut(), "{request}").unwrap();
    }

    pub(crate) fn reject_authentication(&mut self) {
        self.send(&::serde_json::json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {"sql_fun_token": "wrong"},
        }));
        self.assert_closed();
    }

    pub(crate) fn assert_closed(&mut self) {
        let mut response = String::new();
        // Closing a connection with unread bytes may produce a reset.
        match self.stream.read_line(&mut response) {
            Ok(size) => assert_eq!(size, 0, "unexpected response: {response}"),
            Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset),
        }
    }
}
