use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

use crate::{Args, ServerError};
use ::sql_fun_core::RegistrationLock;

#[derive(serde::Deserialize, serde::Serialize, derive_getters::Getters)]
#[allow(clippy::struct_field_names)]
pub struct PidFile {
    pid: u32,
    port: u16,
    token: String,

    #[serde(skip)]
    pid_file_path: Option<PathBuf>,
    #[serde(skip)]
    pid_file: Option<File>,
}

impl Drop for PidFile {
    fn drop(&mut self) {
        if let (Some(path), Some(file)) = (&self.pid_file_path, &self.pid_file) {
            // Retain the registration's shared lock while serializing removal.
            if let Ok(lock) = RegistrationLock::open(path)
                && let Ok(_guard) = lock.lock()
                && matches!(same_file_at_path(file, path), Ok(true))
            {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

#[cfg(test)]
mod drop_tests {
    use crate::test_helpers::pid_file::PidFileFixture;

    #[test]
    fn owner_drop_preserves_replacement() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let owner = fixture.register(30000)?.expect("primary");
        fixture.replace_with_malformed_primary()?;
        drop(owner);
        assert!(fixture.exists());
        assert!(fixture.read().is_err());
        Ok(())
    }

    #[test]
    fn owned_file_is_removed_on_drop_and_can_be_registered_again() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let owner = fixture.register(30000)?.expect("primary");
        drop(fixture.read()?);
        assert!(fixture.exists());
        drop(owner);
        assert!(!fixture.exists());
        assert!(fixture.register(30001)?.is_some());
        Ok(())
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

    fn from_open_file(file: &mut File) -> Result<Self, ServerError> {
        file.seek(SeekFrom::Start(0))?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn process_exists(&self) -> bool {
        let mut sys = sysinfo::System::new();
        sys.refresh_specifics(
            sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::default()),
        );
        sys.process(sysinfo::Pid::from_u32(self.pid)).is_some()
    }
}

// Identifies the inode while its open handle prevents inode reuse. This is not
// an atomic compare-and-delete against processes that ignore the lock protocol.
#[cfg(unix)]
fn same_file_at_path(file: &File, path: &Path) -> std::io::Result<bool> {
    let observed = file.metadata()?;
    let current = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    Ok(observed.dev() == current.dev() && observed.ino() == current.ino())
}

#[cfg(windows)]
fn same_file_at_path(file: &File, path: &Path) -> std::io::Result<bool> {
    let observed = ::same_file::Handle::from_file(file.try_clone()?)?;
    let current = match ::same_file::Handle::from_path(path) {
        Ok(handle) => handle,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    Ok(observed == current)
}

// A temporary name belongs to this attempt only. The guard also covers early
// returns and unwinding; explicit removal reports ordinary cleanup errors.
struct TemporaryName(Option<PathBuf>);

impl TemporaryName {
    fn create(parent: &Path) -> std::io::Result<(Self, File)> {
        loop {
            let random: u128 = ::rand::random();
            let path = parent.join(format!(".sql-fun-server-{random:032x}.tmp"));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(&path) {
                Ok(file) => return Ok((Self(Some(path)), file)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }

    fn path(&self) -> &Path {
        self.0.as_deref().expect("temporary name is present")
    }

    fn remove(&mut self) -> std::io::Result<()> {
        if let Some(path) = &self.0 {
            std::fs::remove_file(path)?;
            self.0 = None;
        }
        Ok(())
    }
}

impl Drop for TemporaryName {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

impl PidFile {
    fn write_to_path(&mut self, path: &Path) -> Result<(), ServerError> {
        self.write_to_path_with(path, Write::write_all, TemporaryName::remove)
    }

    fn write_to_path_with(
        &mut self,
        path: &Path,
        write: impl FnOnce(&mut File, &[u8]) -> std::io::Result<()>,
        remove_temp: impl FnOnce(&mut TemporaryName) -> std::io::Result<()>,
    ) -> Result<(), ServerError> {
        if self.pid_file.is_some() || self.pid_file_path.is_some() {
            return Err(ServerError::Bug(String::from("pid file already persisted")));
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)?;
        let lock = RegistrationLock::open(path)?;
        let _guard = lock.lock()?;
        let content = serde_json::to_vec(self)?;
        let (mut name, mut file) = TemporaryName::create(parent)?;

        let preparation = write(&mut file, &content)
            .and_then(|()| file.try_lock_shared().map_err(std::io::Error::from))
            .and_then(|()| std::fs::hard_link(name.path(), path));
        if let Err(error) = preparation {
            return match name.remove() {
                Ok(()) => Err(error.into()),
                Err(cleanup) => Err(std::io::Error::other(format!(
                    "{error}; temporary PID file cleanup failed: {cleanup}"
                ))
                .into()),
            };
        }

        if let Err(error) = remove_temp(&mut name) {
            use std::fmt::Write as _;
            let rollback = match same_file_at_path(&file, path) {
                Ok(true) => std::fs::remove_file(path),
                Ok(false) => Ok(()),
                Err(error) => Err(error),
            };
            let cleanup = name.remove();
            let mut message = format!("temporary PID file cleanup failed: {error}");
            if let Err(rollback) = rollback {
                let _ = write!(message, "; published PID file rollback failed: {rollback}");
            }
            if let Err(cleanup) = cleanup {
                let _ = write!(
                    message,
                    "; temporary PID file cleanup retry failed: {cleanup}"
                );
            }
            return Err(std::io::Error::other(message).into());
        }
        self.pid_file = Some(file);
        self.pid_file_path = Some(path.to_path_buf());
        Ok(())
    }
}

#[cfg(test)]
mod publication_tests {
    use crate::{
        pid_file::{PidFile, TemporaryName},
        test_helpers::pid_file::PidFileFixture,
    };
    use std::io::Write;

    #[test]
    fn publishes_only_after_complete_write_and_removes_temporary_name() -> ::testresult::TestResult
    {
        let fixture = PidFileFixture::new()?;
        let mut owner = PidFile::new(30000);
        let ready = std::sync::Barrier::new(2);
        let observed = std::sync::Barrier::new(2);
        std::thread::scope(|scope| -> ::testresult::TestResult {
            let reader = scope.spawn(|| {
                ready.wait();
                let absent = !fixture.exists();
                observed.wait();
                absent
            });
            owner.write_to_path_with(
                fixture.path(),
                |file, content| {
                    let split = content.len() / 2;
                    file.write_all(&content[..split])?;
                    ready.wait();
                    observed.wait();
                    file.write_all(&content[split..])
                },
                TemporaryName::remove,
            )?;
            assert!(reader.join().expect("reader thread"));
            Ok(())
        })?;
        let record = fixture.read()?;
        assert_eq!(*record.pid(), std::process::id());
        assert_eq!(*record.port(), 30000);
        assert_eq!(record.token(), owner.token());
        assert_eq!(owner.token().len(), 40);
        assert_eq!(fixture.names()?, vec![fixture.path().to_path_buf()]);
        Ok(())
    }

    #[test]
    fn failed_write_never_publishes_and_cleans_temporary_name() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let mut owner = PidFile::new(30000);
        let result = owner.write_to_path_with(
            fixture.path(),
            |file, content| {
                file.write_all(&content[..1])?;
                Err(std::io::Error::other("injected write failure"))
            },
            TemporaryName::remove,
        );
        assert!(result.is_err());
        assert!(!fixture.exists());
        assert_eq!(fixture.names()?.len(), 0);
        Ok(())
    }

    #[test]
    fn failed_publication_preserves_existing_file_and_cleans_temporary_name()
    -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let owner = fixture.register(30000)?.expect("primary");
        let mut other = PidFile::new(30001);
        assert!(other.write_to_path(fixture.path()).is_err());
        assert_eq!(fixture.read()?.token(), owner.token());
        assert_eq!(fixture.names()?, vec![fixture.path().to_path_buf()]);
        Ok(())
    }

    #[test]
    fn failed_temporary_unlink_rolls_back_publication() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let mut owner = PidFile::new(30000);
        let error = match owner.write_to_path_with(fixture.path(), Write::write_all, |_name| {
            Err(std::io::Error::other("injected unlink failure"))
        }) {
            Ok(()) => panic!("unlink should fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("injected unlink failure"));
        assert!(!fixture.exists());
        assert_eq!(fixture.names()?.len(), 0);
        Ok(())
    }

    #[test]
    fn reports_cleanup_retry_failure_after_publication() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let mut owner = PidFile::new(30000);
        let error = match owner.write_to_path_with(fixture.path(), Write::write_all, |name| {
            std::fs::remove_file(name.path())?;
            Err(std::io::Error::other("injected unlink failure"))
        }) {
            Ok(()) => panic!("unlink should fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("cleanup retry failed"));
        assert!(!fixture.exists());
        assert_eq!(fixture.names()?.len(), 0);
        Ok(())
    }

    #[test]
    fn lock_is_held_before_temporary_name_is_removed() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let mut owner = PidFile::new(30000);
        owner.write_to_path_with(fixture.path(), Write::write_all, |name| {
            let observer = fixture.open_lock_observer()?;
            let error = std::io::Error::from(observer.try_lock().expect_err("already locked"));
            assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
            name.remove()
        })?;
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
        observer.try_lock()?;
        observer.unlock()?;
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

impl PidFile {
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

    pub(crate) fn try_register_at_path(
        pid_path: &Path,
        port: u16,
    ) -> Result<Option<Self>, ServerError> {
        loop {
            let mut observed = match File::open(pid_path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    // A dangling symlink makes open return NotFound but still
                    // prevents hard-link creation; retrying it would never end.
                    if std::fs::symlink_metadata(pid_path)
                        .is_ok_and(|metadata| metadata.file_type().is_symlink())
                    {
                        return Err(error.into());
                    }
                    let mut pid_file = Self::new(port);
                    match pid_file.write_to_path(pid_path) {
                        Ok(()) => return Ok(Some(pid_file)),
                        Err(ServerError::Io(error))
                            if error.kind() == std::io::ErrorKind::AlreadyExists =>
                        {
                            continue;
                        }
                        Err(error) => return Err(error),
                    }
                }
                Err(error) => return Err(error.into()),
            };
            if Self::from_open_file(&mut observed)?.process_exists() {
                return Ok(None);
            }
            if Self::remove_if_stale(pid_path, &mut observed)? {
                return Ok(None);
            }
        }
    }
}

#[cfg(test)]
mod registration_tests {
    use crate::test_helpers::pid_file::PidFileFixture;
    use std::sync::Barrier;

    #[test]
    fn rejects_live_primary_and_preserves_record() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        let owner = fixture.register(30000)?.expect("primary");
        assert!(fixture.register(30001)?.is_none());
        assert_eq!(fixture.read()?.token(), owner.token());
        assert_eq!(fixture.names()?.len(), 1);
        Ok(())
    }

    #[test]
    fn concurrent_fresh_and_stale_registration_has_one_winner() -> ::testresult::TestResult {
        for stale in [false, true] {
            let fixture = PidFileFixture::new()?;
            if stale {
                fixture.persist_exited_primary()?;
            }
            let barrier = Barrier::new(3);
            std::thread::scope(|scope| -> ::testresult::TestResult {
                let a = scope.spawn(|| {
                    barrier.wait();
                    fixture.register(30000)
                });
                let b = scope.spawn(|| {
                    barrier.wait();
                    fixture.register(30001)
                });
                barrier.wait();
                let a = a.join().expect("first thread")?;
                let b = b.join().expect("second thread")?;
                assert_ne!(a.is_some(), b.is_some());
                let winner = a.as_ref().or(b.as_ref()).expect("winner");
                let record = fixture.read()?;
                assert_eq!(record.token(), winner.token());
                assert_eq!(record.port(), winner.port());
                assert_eq!(fixture.names()?.len(), 1);
                Ok(())
            })?;
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn dangling_registration_symlink_is_reported_without_retrying_forever()
    -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        fixture.persist_dangling_symlink()?;
        assert!(fixture.register(30000).is_err());
        assert_eq!(fixture.names()?.len(), 1);
        Ok(())
    }

    #[test]
    fn malformed_registration_is_reported_and_preserved() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        fixture.persist_malformed_primary()?;
        assert!(fixture.register(30000).is_err());
        assert_eq!(fixture.names()?.len(), 1);
        assert!(fixture.exists());
        Ok(())
    }
}

impl PidFile {
    // Returns true if the observed registration still belongs to a live PID.
    fn remove_if_stale(path: &Path, observed: &mut File) -> Result<bool, ServerError> {
        // Competing removers serialize on the old inode, then recheck its name.
        observed.lock()?;
        let lock = RegistrationLock::open(path)?;
        let _guard = lock.lock()?;
        if !same_file_at_path(observed, path)? {
            return Ok(false);
        }
        if Self::from_open_file(observed)?.process_exists() {
            return Ok(true);
        }
        match std::fs::remove_file(path) {
            Ok(()) => Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod stale_removal_tests {
    use crate::{pid_file::PidFile, test_helpers::pid_file::PidFileFixture};
    use std::fs::File;

    #[test]
    fn old_observation_cannot_remove_a_replacement() -> ::testresult::TestResult {
        let fixture = PidFileFixture::new()?;
        fixture.persist_exited_primary()?;
        let mut first = File::open(fixture.path())?;
        let mut second = File::open(fixture.path())?;
        assert!(!PidFile::remove_if_stale(fixture.path(), &mut first)?);
        drop(first);
        let owner = fixture.register(30001)?.expect("replacement");
        assert!(!PidFile::remove_if_stale(fixture.path(), &mut second)?);
        assert_eq!(fixture.read()?.token(), owner.token());
        Ok(())
    }
}
