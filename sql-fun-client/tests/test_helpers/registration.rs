use std::path::{Path, PathBuf};
use std::time::Duration;

/// Lightweight isolated registration inputs; never builds or starts a server.
pub struct RegistrationFixture {
    directory: ::tempfile::TempDir,
}

impl RegistrationFixture {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            directory: ::tempfile::Builder::new()
                .prefix("sql-fun-record-")
                .tempdir_in("/tmp")?,
        })
    }

    pub fn path(&self) -> PathBuf {
        self.directory.path().join("sql-fun-server.pid")
    }

    pub fn write(&self, record: &[u8]) -> std::io::Result<()> {
        std::fs::write(self.path(), record)
    }

    pub fn preserve_unusable_record(&self) -> std::io::Result<Vec<u8>> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let record = ::serde_json::to_vec(&::serde_json::json!({
            "pid": std::process::id(), "port": listener.local_addr()?.port(), "token": "fixture-secret"
        }))?;
        self.write(&record)?;
        Ok(record)
    }

    pub fn options(&self, timeout: Duration) -> ::sql_fun_client::ConnectionOptions {
        ::sql_fun_client::ConnectionOptions::new(
            Path::new("/nonexistent-sql-fun-server"),
            self.directory.path().file_name().unwrap().to_str().unwrap(),
            Some("exit 97"),
            Some(timeout),
        )
    }

    pub fn spawn_failure_options(&self) -> ::sql_fun_client::ConnectionOptions {
        ::sql_fun_client::ConnectionOptions::new(
            Path::new("/nonexistent-sql-fun-server"),
            self.directory.path().file_name().unwrap().to_str().unwrap(),
            None,
            Some(Duration::from_secs(1)),
        )
    }
}
