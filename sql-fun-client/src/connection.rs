use crate::{TcpJsonRpcClientAsync, async_tokio::DEFAULT_MAX_RESPONSE_BYTES, recovery::recover};

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

    pub(crate) fn failure(stage: &str, message: &str) -> Self {
        Self {
            message: format!("{stage}: {message}"),
            timed_out: false,
        }
    }

    pub(crate) fn timeout(path: &Path, pid: Option<u32>, last: &str) -> Self {
        let action = if pid.is_some() {
            "Check the registered server and retry with a sufficient timeout"
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
pub(crate) struct Registration {
    pub(crate) pid: u32,
    port: u16,
    token: String,
    #[serde(skip)]
    file: Option<std::fs::File>,
}

impl Registration {
    async fn read(path: &Path) -> Result<Option<Self>, ConnectionError> {
        use ::tokio::io::AsyncReadExt;
        let mut file = match ::tokio::fs::File::open(path).await {
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
        (&mut file)
            .take(u64::from(REGISTRATION_LIMIT) + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| ConnectionError::failure("registration", &error.to_string()))?;
        // Never include JSON errors or raw records: they can contain the token.
        let mut record: Self = ::serde_json::from_slice(&bytes).map_err(|_| {
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
        record.file = Some(file.into_std().await);
        Ok(Some(record))
    }

    pub(crate) fn still_at(&self, path: &Path) -> std::io::Result<bool> {
        use std::{io::Read, os::unix::fs::MetadataExt};
        let old = self
            .file
            .as_ref()
            .expect("observed registration")
            .metadata()?;
        let current = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if !current.is_file() || old.dev() != current.dev() || old.ino() != current.ino() {
            return Ok(false);
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(u64::from(REGISTRATION_LIMIT) + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > usize::from(REGISTRATION_LIMIT) {
            return Ok(false);
        }
        let Ok(record) = ::serde_json::from_slice::<Self>(&bytes) else {
            return Ok(false);
        };
        Ok(self.pid == record.pid && self.port == record.port && self.token == record.token)
    }

    fn same_record(&self, other: &Self) -> bool {
        self.pid == other.pid
            && self.port == other.port
            && self.token == other.token
            && self
                .file
                .as_ref()
                .zip(other.file.as_ref())
                .is_some_and(|(a, b)| {
                    use std::os::unix::fs::MetadataExt;
                    a.metadata()
                        .ok()
                        .zip(b.metadata().ok())
                        .is_some_and(|(a, b)| a.dev() == b.dev() && a.ino() == b.ino())
                })
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

struct ConnectionAttempt {
    child: Option<Child>,
    launched: bool,
    observed: Option<(Registration, Instant)>,
    last: String,
    readiness_budget: Duration,
}

impl ConnectionAttempt {
    fn new(deadline: Instant) -> Self {
        Self {
            child: None,
            launched: false,
            observed: None,
            last: String::from("registration not published"),
            readiness_budget: deadline.saturating_duration_since(Instant::now()) / 3,
        }
    }

    fn pid(&self) -> Option<u32> {
        self.observed.as_ref().map(|(record, _)| record.pid)
    }

    async fn try_registration(
        &mut self,
        record: Registration,
        options: &ConnectionOptions,
        path: &Path,
        deadline: Instant,
    ) -> Result<Option<TcpJsonRpcClientAsync>, ConnectionError> {
        let since = match self.observed.take() {
            Some((previous, since)) if previous.same_record(&record) => since,
            _ => Instant::now(),
        };
        let readiness_deadline = since
            .checked_add(self.readiness_budget)
            .unwrap_or(deadline)
            .min(deadline);
        let attempt_deadline = readiness_deadline.min(Instant::now() + ATTEMPT_TIMEOUT);
        if Instant::now() < attempt_deadline {
            match record
                .authenticate(attempt_deadline, options.max_response_bytes)
                .await
            {
                Ok(client) if Instant::now() < deadline => return Ok(Some(client)),
                Ok(_) => self.last = String::from("initialize completed after connection deadline"),
                Err(error) => self.last = error,
            }
        }
        if Instant::now() >= readiness_deadline {
            recover(
                path,
                &record,
                &options.server_executable,
                deadline,
                &self.last,
            )
            .await?;
            // Discovery decides whether to follow a replacement or launch.
            self.launched = false;
        } else {
            self.observed = Some((record, since));
        }
        Ok(None)
    }

    fn launch_if_absent(&mut self, options: &ConnectionOptions) -> Result<(), ConnectionError> {
        if self.observed.take().is_some() {
            self.launched = false;
        }
        if !self.launched {
            self.child = Some(options.launch()?);
            self.launched = true;
        }
        Ok(())
    }

    fn observe_launch(&mut self) -> Result<(), ConnectionError> {
        if let Some(process) = &mut self.child
            && let Some(status) = process
                .try_wait()
                .map_err(|error| ConnectionError::failure("launch", &error.to_string()))?
        {
            if self.observed.is_none() {
                self.last = format!(
                    "launch exited with {status}; no authenticated primary discovered; previous failure: {}",
                    self.last
                );
            }
            self.child = None;
        }
        Ok(())
    }

    async fn step(
        &mut self,
        options: &ConnectionOptions,
        path: &Path,
        deadline: Instant,
    ) -> Result<Option<TcpJsonRpcClientAsync>, ConnectionError> {
        if Instant::now() >= deadline {
            return Err(ConnectionError::timeout(path, self.pid(), &self.last));
        }
        let record = timeout_at(deadline, Registration::read(path))
            .await
            .map_err(|_| {
                ConnectionError::timeout(path, self.pid(), "registration read stalled")
            })??;
        if Instant::now() >= deadline {
            return Err(ConnectionError::timeout(path, self.pid(), &self.last));
        }
        if let Some(record) = record {
            if let Some(client) = self
                .try_registration(record, options, path, deadline)
                .await?
            {
                return Ok(Some(client));
            }
        } else {
            self.launch_if_absent(options)?;
        }
        self.observe_launch()?;
        Ok(None)
    }
}

/// Discover, launch if absent, and authenticate a per-user server on localhost.
///
/// Unusable registrations are recovered only when process identity can be
/// verified safely. Drop or cancellation closes client sockets, without initiating
/// termination. All retries and recovery share a single deadline.
///
/// # Errors
/// Reports invalid options, malformed registration, unsafe process identity,
/// launch failures, and deadline expiration with the last readiness failure.
pub async fn connect(
    options: &ConnectionOptions,
) -> Result<TcpJsonRpcClientAsync, ConnectionError> {
    let deadline = options.deadline()?;
    let path = options.registration_path();
    let mut attempt = ConnectionAttempt::new(deadline);
    loop {
        if let Some(client) = attempt.step(options, &path, deadline).await? {
            return Ok(client);
        }
        sleep_until(deadline.min(Instant::now() + RETRY_INTERVAL)).await;
    }
}
