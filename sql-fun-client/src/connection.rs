use crate::{TcpJsonRpcClientAsync, async_tokio::DEFAULT_MAX_RESPONSE_BYTES};

use std::{
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use ::tokio::{
    process::{Child, Command},
    time::{Instant, sleep_until, timeout_at},
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(40);
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_millis(50);
const REGISTRATION_LIMIT: u16 = 8192;

/// Explicit inputs to per-user discovery and optional server launch.
///
/// The user identifies `/tmp/<user>/sql-fun-server.pid`, and must be one nonempty
/// path component. No project metadata or executable is inferred.
///
/// Prefer the builder: executable and user are required at compile time, while
/// command, timeout, and response limit may be omitted. Omitted timeout means
/// 40 seconds; the response limit defaults to 4 MiB. Use the builder's
/// `max_response_bytes` setter for a `u32` byte limit, including the newline.
/// A receive-size failure closes the connection; reconnect with a larger limit
/// to retry a valid large response. This does not change the authentication bound.
///
/// ```
/// use sql_fun_client::ConnectionOptions;
/// let options = ConnectionOptions::builder()
///     .server_executable("/opt/sql-fun/bin/sql-fun-server")
///     .user("alice")
///     .build();
/// ```
///
/// Both choices must be supplied before building:
///
/// ```compile_fail
/// use sql_fun_client::ConnectionOptions;
/// ConnectionOptions::builder().user("alice").build();
/// ```
///
/// ```compile_fail
/// use sql_fun_client::ConnectionOptions;
/// ConnectionOptions::builder().server_executable("/server").build();
/// ```
#[derive(Clone, bon::Builder)]
pub struct ConnectionOptions {
    /// Required absolute path of the expected server executable.
    #[builder(into)]
    server_executable: PathBuf,
    /// Required explicit user namespace under `/tmp`.
    #[builder(into)]
    user: String,
    /// Optional launch override, executed by `/bin/sh -c`.
    #[builder(into)]
    server_command: Option<String>,
    /// Optional positive total connection budget; omission means 40 seconds.
    timeout: Option<Duration>,
    /// Maximum authenticated response frame bytes, including the terminating newline.
    /// Defaults to 4 MiB. Zero rejects all responses; the authentication bound is separate.
    #[builder(default = DEFAULT_MAX_RESPONSE_BYTES)]
    max_response_bytes: u32,
}

impl ConnectionOptions {
    /// Convenience constructor; prefer [`Self::builder`] for named options.
    ///
    /// `None` timeout means 40 seconds. The executable must be absolute. A command
    /// override runs through `/bin/sh -c` in the caller's current directory and
    /// inherits its environment, with `USER` set to the explicitly selected user.
    /// These choices are validated by [`connect`] before filesystem access.
    #[must_use]
    pub fn new(
        server_executable: &Path,
        user: &str,
        server_command: Option<&str>,
        timeout: Option<Duration>,
    ) -> Self {
        Self {
            server_executable: server_executable.to_owned(),
            user: user.to_owned(),
            server_command: server_command.map(str::to_owned),
            timeout,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }

    fn deadline(&self) -> Result<Instant, ConnectionError> {
        let started = Instant::now();
        let timeout = self.timeout.unwrap_or(DEFAULT_TIMEOUT);
        if timeout.is_zero() || !self.server_executable.is_absolute() {
            return Err(ConnectionError::invalid(
                "timeout must be positive and server executable must be absolute",
            ));
        }
        if self.user.is_empty()
            || self.user == "."
            || self.user == ".."
            || self.user.contains('/')
            || self.user.contains('\0')
        {
            return Err(ConnectionError::invalid(
                "user must be one nonempty path component",
            ));
        }
        started
            .checked_add(timeout)
            .ok_or_else(|| ConnectionError::invalid("timeout is too large"))
    }

    fn registration_path(&self) -> PathBuf {
        Path::new("/tmp")
            .join(&self.user)
            .join("sql-fun-server.pid")
    }

    fn launch(&self) -> Result<Child, ConnectionError> {
        let mut command = if let Some(shell_command) = &self.server_command {
            let mut command = Command::new("/bin/sh");
            command.arg("-c").arg(shell_command);
            command
        } else {
            Command::new(&self.server_executable)
        };
        command
            .env("USER", &self.user)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(false);
        // SAFETY: the child hook performs only the async-signal-safe setsid
        // syscall, without allocation, locks, or access to parent state.
        #[allow(unsafe_code)]
        unsafe {
            command.as_std_mut().pre_exec(|| {
                ::nix::unistd::setsid()
                    .map(|_| ())
                    .map_err(std::io::Error::from)
            });
        }
        command.spawn().map_err(|error| {
            ConnectionError::failure("launch", &format!("cannot spawn server: {error}"))
        })
    }
}

#[cfg(test)]
mod test_options {
    use crate::connection::connect;

    use crate::connection::{ConnectionOptions, DEFAULT_TIMEOUT};
    use std::{path::Path, time::Duration};

    #[test]
    fn default_timeout_is_forty_seconds() {
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(40));
        let options = ConnectionOptions::builder()
            .server_executable(Path::new("/server"))
            .user("test")
            .build();
        let before = ::tokio::time::Instant::now();
        let deadline = options.deadline().unwrap();
        assert!(deadline >= before + DEFAULT_TIMEOUT);
        assert!(deadline < before + DEFAULT_TIMEOUT + Duration::from_secs(1));
    }

    #[rstest::rstest]
    #[case::omitted(None, None)]
    #[case::overridden(Some("cargo run -p sql-fun-server"), Some(Duration::from_secs(3)))]
    fn builder_accepts_optional_borrowed_inputs(
        #[case] command: Option<&str>,
        #[case] timeout: Option<Duration>,
    ) {
        let options = ConnectionOptions::builder()
            .server_executable(Path::new("/server"))
            .user("test")
            .maybe_server_command(command)
            .maybe_timeout(timeout)
            .build();
        assert_eq!(options.server_executable, Path::new("/server"));
        assert_eq!(options.user, "test");
        assert_eq!(options.server_command.as_deref(), command);
        assert_eq!(options.timeout, timeout);
    }

    #[rstest::rstest]
    #[case::zero("/server", "test", Duration::ZERO)]
    #[case::relative("server", "test", Duration::from_secs(1))]
    #[case::empty_user("/server", "", Duration::from_secs(1))]
    #[case::parent_user("/server", "..", Duration::from_secs(1))]
    #[case::nested_user("/server", "a/b", Duration::from_secs(1))]
    #[tokio::test]
    async fn invalid_options_fail_before_launch(
        #[case] executable: &str,
        #[case] user: &str,
        #[case] timeout: Duration,
    ) {
        let options = ConnectionOptions::builder()
            .server_executable(executable)
            .user(user)
            .server_command("exit 97")
            .timeout(timeout)
            .build();
        let error = connect(&options).await.unwrap_err();
        assert!(error.to_string().starts_with("invalid input:"));
        assert!(!error.is_timeout());
    }
}

/// A connection failure with redacted, stage-specific diagnostics.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ConnectionError {
    message: String,
    timed_out: bool,
}

impl ConnectionError {
    fn invalid(message: &str) -> Self {
        Self::failure("invalid input", message)
    }

    fn failure(stage: &str, message: &str) -> Self {
        Self {
            message: format!("{stage}: {message}"),
            timed_out: false,
        }
    }

    fn timeout(path: &Path, pid: Option<u32>, last: &str) -> Self {
        let action = if pid.is_some() {
            "Automatic recovery is unsupported; verify the intended server and registration before operator-assisted cleanup, with other startups stopped"
        } else {
            "Check the selected executable and startup command; no usable registration was discovered"
        };
        Self {
            message: format!(
                "connection timeout at {} (registered PID {pid:?}); last failure: {last}. {action}",
                path.display()
            ),
            timed_out: true,
        }
    }

    /// Whether the single connection deadline expired.
    #[must_use]
    pub const fn is_timeout(&self) -> bool {
        self.timed_out
    }
}

#[derive(serde::Deserialize)]
struct Registration {
    pid: u32,
    port: u16,
    token: String,
}

impl Registration {
    async fn read(path: &Path) -> Result<Option<Self>, ConnectionError> {
        use ::tokio::io::AsyncReadExt;
        let file = match ::tokio::fs::File::open(path).await {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(ConnectionError::failure(
                    "registration",
                    &format!("{}: {error}", path.display()),
                ));
            }
        };
        let mut bytes = Vec::new();
        file.take(u64::from(REGISTRATION_LIMIT) + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| ConnectionError::failure("registration", &error.to_string()))?;
        // Never include JSON errors or raw records: they can contain the token.
        let record: Self = ::serde_json::from_slice(&bytes).map_err(|_| {
            ConnectionError::failure(
                "registration",
                &format!(
                    "{}: malformed or incomplete record; file preserved",
                    path.display()
                ),
            )
        })?;
        if bytes.len() > usize::from(REGISTRATION_LIMIT)
            || record.pid == 0
            || record.port == 0
            || record.token.is_empty()
        {
            return Err(ConnectionError::failure(
                "registration",
                &format!(
                    "{}: invalid required fields; file preserved",
                    path.display()
                ),
            ));
        }
        Ok(Some(record))
    }

    async fn authenticate(
        &self,
        deadline: Instant,
        max_response_bytes: u32,
    ) -> Result<TcpJsonRpcClientAsync, String> {
        let stream = timeout_at(
            deadline,
            ::tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, self.port)),
        )
        .await
        .map_err(|_| String::from("TCP connect timed out"))?
        .map_err(|error| format!("TCP connect: {error}"))?;
        let mut client =
            TcpJsonRpcClientAsync::try_from_with_max_response_bytes(stream, max_response_bytes)
                .map_err(|error| format!("TCP setup: {error}"))?;
        let response = timeout_at(
            deadline,
            client.invoke_bounded(
                "initialize",
                &::serde_json::json!({"sql_fun_token": self.token}),
                8192,
            ),
        )
        .await
        .map_err(|_| String::from("initialize write/read timed out"))?
        .map_err(|error| error.initialization_diagnostic())?;
        if response != "OK" {
            return Err(String::from("initialize: expected successful result OK"));
        }
        Ok(client)
    }
}

/// Discover, launch if absent, and authenticate a per-user server on localhost.
///
/// This Linux operation never deletes registrations or signals registered servers.
/// An unusable registration requires operator-assisted recovery. Drop/cancellation
/// closes client sockets; detached servers persist. All retries share one deadline.
///
/// # Errors
/// Rejects invalid options before filesystem access. Reports malformed registration,
/// spawn failure, or deadline expiration with the last readiness failure. An existing
/// registration is preserved even when its server is dead or authentication fails.
pub async fn connect(
    options: &ConnectionOptions,
) -> Result<TcpJsonRpcClientAsync, ConnectionError> {
    let deadline = options.deadline()?;
    let path = options.registration_path();
    let mut child: Option<Child> = None;
    let mut launched = false;
    let mut observed_registration = false;
    let mut pid = None;
    let mut last = String::from("registration not published");
    loop {
        if Instant::now() >= deadline {
            return Err(ConnectionError::timeout(&path, pid, &last));
        }
        let record = timeout_at(deadline, Registration::read(&path))
            .await
            .map_err(|_| ConnectionError::timeout(&path, pid, "registration read stalled"))??;
        if Instant::now() >= deadline {
            return Err(ConnectionError::timeout(&path, pid, &last));
        }
        if let Some(record) = record {
            observed_registration = true;
            pid = Some(record.pid);
            let attempt_deadline = deadline.min(Instant::now() + ATTEMPT_TIMEOUT);
            match record
                .authenticate(attempt_deadline, options.max_response_bytes)
                .await
            {
                Ok(client) if Instant::now() < deadline => return Ok(client),
                Ok(_) => last = String::from("initialize completed after connection deadline"),
                Err(error) => last = error,
            }
        } else if !launched && !observed_registration {
            // Publication arbitrates the race with another fresh-start client.
            // Never launch again after observing any existing registration.
            child = Some(options.launch()?);
            launched = true;
        }
        if let Some(process) = &mut child
            && let Some(status) = process
                .try_wait()
                .map_err(|error| ConnectionError::failure("launch", &error.to_string()))?
        {
            // A loser may exit with either status while the winner publishes.
            // Continue discovery until the same deadline, retaining evidence.
            if !observed_registration {
                last = format!("launch exited with {status}; no authenticated primary discovered");
            }
            child = None;
        }
        sleep_until(deadline.min(Instant::now() + RETRY_INTERVAL)).await;
    }
}
