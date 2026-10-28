use std::{
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

/// Starts an isolated child for real argument parsing, without schema setup.
/// Each invocation owns its namespace and bounds the child lifetime so a parser
/// regression cannot leak a server. Drop kills/reaps any remaining child.
pub(crate) struct Server {
    child: Option<Child>,
    namespace: ::tempfile::TempDir,
}

impl Server {
    fn new() -> Self {
        Self {
            child: None,
            namespace: ::tempfile::Builder::new()
                .prefix("sql-fun-idle-")
                .tempdir_in("/tmp")
                .unwrap(),
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
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
