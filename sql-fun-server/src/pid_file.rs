#[cfg(test)]
mod test_helpers;

use std::path::{Path, PathBuf};

#[cfg(not(target_os = "windows"))]
use std::os::unix::fs::OpenOptionsExt;

use crate::{Args, ServerError};

#[derive(serde::Deserialize, serde::Serialize, derive_getters::Getters)]
#[allow(clippy::struct_field_names)]
pub struct PidFile {
    pid: u32,
    port: u16,
    token: String,

    #[serde(skip)]
    pid_file_path: Option<PathBuf>,
    #[serde(skip)]
    pid_file: Option<std::fs::File>,
}

impl Drop for PidFile {
    fn drop(&mut self) {
        if let Some(pid_file_path) = &self.pid_file_path {
            let _ = std::fs::remove_file(pid_file_path); // ignore error
        }
    }
}

impl PidFile {
    fn new(port: u16) -> Self {
        let mut arr = [0u8; 20];
        ::rand::fill(&mut arr[..]);
        let mut token = String::new();
        for b in &arr {
            use std::fmt::Write as _;
            let _ = write!(token, "{b:02x}");
        }
        Self {
            pid: std::process::id(),
            pid_file_path: None,
            pid_file: None,
            token,
            port,
        }
    }

    fn default_path(args: &Args) -> PathBuf {
        let mut pid_path = PathBuf::from("/tmp");
        pid_path.push(&args.user);
        pid_path.push("sql-fun-server.pid");
        pid_path
    }

    fn from_file(path: &Path) -> Result<Self, ServerError> {
        let pid_file_content = std::fs::read_to_string(path)?;
        let pid_file: Self = serde_json::from_str(&pid_file_content)?;
        Ok(pid_file)
    }

    fn process_exists(&self) -> bool {
        let mut sys = sysinfo::System::new();
        sys.refresh_specifics(
            sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::default()),
        );
        sys.process(sysinfo::Pid::from_u32(self.pid)).is_some()
    }

    fn write_to_path(&mut self, path: &Path) -> Result<(), ServerError> {
        use std::fs::OpenOptions;
        use std::io::Write;

        if self.pid_file.is_some() || self.pid_file_path.is_some() {
            return Err(ServerError::Bug(String::from("pid file ready persisted")));
        }
        // make sure create parent dir
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string(self)?;

        #[cfg(not(target_os = "windows"))]
        let mut file = OpenOptions::new()
            .mode(0o600)
            .write(true)
            .create_new(true)
            .open(path)?;

        #[cfg(target_os = "windows")]
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;

        file.write_all(content.as_bytes())?;
        file.try_lock_shared().map_err(std::io::Error::from)?;

        self.pid_file = Some(file);
        self.pid_file_path = Some(path.to_path_buf());
        Ok(())
    }

    /// Attempts to register this process as the primary server.
    ///
    /// # Returns
    ///
    /// - `None`: Another process is already running as the primary server. This process should exit.
    /// - `Some`: This process has successfully registered as the primary server.
    ///
    pub fn try_register_primary_server(
        args: &Args,
        port: u16,
    ) -> Result<Option<Self>, ServerError> {
        Self::try_register_at_path(&Self::default_path(args), port)
    }

    fn try_register_at_path(pid_path: &Path, port: u16) -> Result<Option<Self>, ServerError> {
        if pid_path.exists() {
            let pid_file = Self::from_file(pid_path)?;
            if pid_file.process_exists() {
                return Ok(None);
            }
            std::fs::remove_file(pid_path)?;
        }
        let mut pid_file = Self::new(port);
        pid_file.write_to_path(pid_path)?;
        Ok(Some(pid_file))
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::PidFileFixture;

    #[test]
    fn registers_and_persists_server_identity() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let owner = fixture.register(30000)?.expect("primary registration");
        let persisted = fixture.read()?;
        assert_eq!(*persisted.pid(), std::process::id());
        assert_eq!(*persisted.port(), 30000);
        assert_eq!(persisted.token(), owner.token());
        assert_eq!(owner.token().len(), 40);
        Ok(())
    }

    #[test]
    fn rejects_live_primary_without_changing_its_file() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let owner = fixture.register(30000)?.expect("primary registration");
        assert!(fixture.register(30001)?.is_none());
        let persisted = fixture.read()?;
        assert_eq!(*persisted.port(), 30000);
        assert_eq!(persisted.token(), owner.token());
        Ok(())
    }

    #[test]
    fn replaces_stale_primary() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let stale = fixture.persist_exited_primary()?;
        assert!(!stale.process_exists());
        let owner = fixture.register(30001)?.expect("replacement registration");
        let persisted = fixture.read()?;
        assert_eq!(*persisted.pid(), std::process::id());
        assert_eq!(*persisted.port(), 30001);
        assert_eq!(persisted.token(), owner.token());
        Ok(())
    }

    #[test]
    fn holds_shared_lock_until_owner_is_dropped() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let owner = fixture.register(30000)?.expect("primary registration");
        let observer = fixture.open_lock_observer()?;
        observer.try_lock_shared()?;
        observer.unlock()?;
        let error = std::io::Error::from(observer.try_lock().expect_err("owner holds lock"));
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        drop(owner);
        // Keep the original handle: reopening the path could observe a new inode.
        observer.try_lock()?;
        observer.unlock()?;
        Ok(())
    }

    #[test]
    fn removes_owned_file_and_allows_reregistration() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let owner = fixture.register(30000)?.expect("primary registration");
        drop(owner);
        assert!(!fixture.exists());
        let replacement = fixture
            .register(30001)?
            .expect("registration after cleanup");
        assert_eq!(*replacement.port(), 30001);
        Ok(())
    }

    #[test]
    fn reader_does_not_remove_owned_file() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let owner = fixture.register(30000)?.expect("primary registration");
        drop(fixture.read()?);
        assert!(fixture.exists());
        assert_eq!(fixture.read()?.token(), owner.token());
        Ok(())
    }

    #[test]
    fn malformed_pid_file_is_reported_and_preserved() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        fixture.persist_malformed_primary()?;
        assert!(fixture.register(30000).is_err());
        assert!(fixture.exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn restricts_pid_file_permissions() -> ::testresult::TestResult {
        use std::os::unix::fs::PermissionsExt;

        let fixture = PidFileFixture::new()?;
        let _owner = fixture.register(30000)?.expect("primary registration");
        assert_eq!(
            fixture
                .open_lock_observer()?
                .metadata()?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        Ok(())
    }
}
