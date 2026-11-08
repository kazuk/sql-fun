use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Prebuilds the actual server for each test process, then owns one isolated server
/// namespace. Cargo's build cache is shared; process/service fixtures are not.
/// This expensive setup verifies actual server lifecycle, not a mock protocol.
pub struct ServerFixture {
    directory: ::tempfile::TempDir,
    executable: PathBuf,
    user: String,
}

impl ServerFixture {
    pub fn new() -> ::testresult::TestResult<Self> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        // Build with the exact package selection used by the Cargo override.
        // Combining packages changes dependency feature unification and can make
        // cargo run recompile during the measured startup interval.
        let mut build = Command::new(env!("CARGO"));
        build
            .current_dir(root)
            .args(["build", "-p", "sql-fun-server", "--bin", "sql-fun-server"]);
        if cfg!(feature = "sqlite") {
            build.args(["--features", "sqlite"]);
        }
        let output = build.output()?;
        assert!(
            output.status.success(),
            "prebuild: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // Use Cargo's resolved target directory, including CARGO_TARGET_DIR.
        let metadata = Command::new(env!("CARGO"))
            .current_dir(root)
            .args(["metadata", "--no-deps", "--format-version=1"])
            .output()?;
        let metadata: ::serde_json::Value = ::serde_json::from_slice(&metadata.stdout)?;
        let target = Path::new(metadata["target_directory"].as_str().unwrap()).join("debug");
        let directory = ::tempfile::Builder::new()
            .prefix("sql-fun-client-")
            .tempdir_in("/tmp")?;
        let user = directory
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        Ok(Self {
            directory,
            executable: target.join("sql-fun-server"),
            user,
        })
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    pub fn options(&self) -> ::sql_fun_client::ConnectionOptions {
        self.with_command(None, Duration::from_secs(10))
    }

    pub fn with_command(
        &self,
        command: Option<&str>,
        timeout: Duration,
    ) -> ::sql_fun_client::ConnectionOptions {
        ::sql_fun_client::ConnectionOptions::new(
            self.executable(),
            self.user(),
            command,
            Some(timeout),
        )
    }

    pub fn registration_path(&self) -> PathBuf {
        self.directory.path().join("sql-fun-server.pid")
    }

    pub fn registration(&self) -> ::testresult::TestResult<::serde_json::Value> {
        Ok(::serde_json::from_slice(&std::fs::read(
            self.registration_path(),
        )?)?)
    }

    pub fn preserve_unusable_record(&self) -> ::testresult::TestResult<Vec<u8>> {
        // Hold no listener on this reserved port after returning. The recorded
        // process is this test: a destructive client would kill the test runner.
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let record = ::serde_json::to_vec(&::serde_json::json!({
            "pid": std::process::id(), "port": listener.local_addr()?.port(), "token": "fixture-secret"
        }))?;
        std::fs::write(self.registration_path(), &record)?;
        Ok(record)
    }

    pub fn persist_registration(&self, record: &::serde_json::Value) -> std::io::Result<()> {
        use std::io::Write;
        let mut temporary = ::tempfile::NamedTempFile::new_in(self.directory.path())?;
        temporary.write_all(&::serde_json::to_vec(record)?)?;
        temporary
            .persist(self.registration_path())
            .map_err(|error| error.error)?;
        Ok(())
    }

    pub fn assert_detached(&self) -> ::testresult::TestResult {
        let pid = ::nix::unistd::Pid::from_raw(i32::try_from(
            self.registration()?["pid"].as_u64().unwrap(),
        )?);
        assert_eq!(::nix::unistd::getsid(Some(pid))?, pid);
        Ok(())
    }
}

impl Drop for ServerFixture {
    fn drop(&mut self) {
        // Tests may terminate only their actual fixture server. A token/port
        // record alone does not authorize terminating the current test process.
        if let Ok(bytes) = std::fs::read(self.registration_path())
            && let Ok(record) = ::serde_json::from_slice::<::serde_json::Value>(&bytes)
            && let Some(pid) = record["pid"].as_u64()
            && let Ok(pid) = i32::try_from(pid)
            && u32::try_from(pid).ok() != Some(std::process::id())
            && let Ok(executable) = std::fs::read_link(format!("/proc/{pid}/exe"))
            && executable.to_string_lossy().trim_end_matches(" (deleted)")
                == self.executable.to_string_lossy()
        {
            let _ = ::nix::sys::signal::kill(
                ::nix::unistd::Pid::from_raw(pid),
                ::nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
}
