// Reuse the real-client lifecycle fixture, including isolated namespaces and
// guarded cleanup. No production test API is needed.
#[allow(dead_code)]
#[path = "../../../sql-fun-client/tests/test_helpers/heavy.rs"]
mod lifecycle;

use std::{path::Path, process::Stdio, time::Duration};

use ::tokio::io::AsyncWriteExt;

/// Prebuilds a real server for each test process and owns one isolated namespace.
/// Cargo's cache is shared; external service initialization is not shared across
/// tests. This setup verifies authentication, streaming, and server persistence.
pub struct ApiServer(lifecycle::ServerFixture);

impl ApiServer {
    pub fn new() -> ::testresult::TestResult<Self> {
        Ok(Self(lifecycle::ServerFixture::new()?))
    }

    pub fn command(&self) -> ::tokio::process::Command {
        let mut command = ::tokio::process::Command::new(env!("CARGO_BIN_EXE_sql-fun"));
        command
            .args(["api"])
            .env("USER", self.0.user())
            .env("SQL_FUN_SERVER", self.0.executable())
            .env_remove("SQL_FUN_SERVER_COMMAND")
            .env_remove("SQL_FUN_METADATA_FILE")
            .env_remove("RUST_LOG")
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }

    pub fn executable(&self) -> &Path {
        self.0.executable()
    }
    pub fn user(&self) -> &str {
        self.0.user()
    }
    pub fn stop_owned_server(&self) -> ::testresult::TestResult {
        let pid = i32::try_from(self.registration()?["pid"].as_u64().unwrap())?;
        let running_executable = std::fs::read_link(format!("/proc/{pid}/exe"))?;
        assert_eq!(running_executable, self.executable());
        ::nix::sys::signal::kill(
            ::nix::unistd::Pid::from_raw(pid),
            ::nix::sys::signal::Signal::SIGKILL,
        )?;
        Ok(())
    }

    pub fn registration(&self) -> ::testresult::TestResult<::serde_json::Value> {
        self.0.registration()
    }

    pub async fn run(&self, input: &str) -> ::testresult::TestResult<std::process::Output> {
        Self::run_command(self.command(), input).await
    }

    pub async fn run_command(
        mut command: ::tokio::process::Command,
        input: &str,
    ) -> ::testresult::TestResult<std::process::Output> {
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(input.as_bytes()).await?;
        drop(stdin);
        Ok(::tokio::time::timeout(Duration::from_secs(60), child.wait_with_output()).await??)
    }
}
