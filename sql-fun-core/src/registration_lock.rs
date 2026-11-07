//! Coordination of publication and removal of a per-user server registration.

use std::{
    fs::{File, OpenOptions},
    path::Path,
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

/// A persistent coordination inode for a registration path.
///
/// All publishers and removers must hold its exclusive guard while changing the
/// registration name. The coordination file must not be removed while in use.
pub struct RegistrationLock(File);

impl RegistrationLock {
    /// Open the coordination file in the registration's existing directory.
    ///
    /// # Errors
    /// Returns an error if the directory or coordination file is unsafe or
    /// cannot be opened.
    pub fn open(registration: &Path) -> std::io::Result<Self> {
        let parent = registration.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "registration has no parent",
            )
        })?;
        #[cfg(unix)]
        {
            let directory = std::fs::symlink_metadata(parent)?;
            if !directory.is_dir()
                || directory.uid() != ::rustix::process::geteuid().as_raw()
                || directory.mode() & 0o022 != 0
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "unsafe registration directory",
                ));
            }
        }
        let name = registration.file_name().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "registration has no file name",
            )
        })?;
        let mut lock_name = name.to_os_string();
        lock_name.push(".lock");
        let path = parent.join(lock_name);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        options
            .mode(0o600)
            .custom_flags(::rustix::fs::OFlags::NOFOLLOW.bits().cast_signed());
        let file = options.open(&path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "coordination path is not a regular file",
            ));
        }
        #[cfg(unix)]
        if metadata.uid() != ::rustix::process::geteuid().as_raw()
            || metadata.mode() & 0o777 != 0o600
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "unsafe coordination file ownership or permissions",
            ));
        }
        Ok(Self(file))
    }

    /// Acquire the coordination lock without waiting.
    ///
    /// # Errors
    /// Returns `WouldBlock` when another participant owns the lock, or an I/O
    /// error when locking is unavailable.
    pub fn try_lock(&self) -> std::io::Result<RegistrationGuard<'_>> {
        self.0.try_lock().map_err(std::io::Error::from)?;
        Ok(RegistrationGuard(self))
    }

    /// Acquire the coordination lock, waiting for another participant.
    ///
    /// # Errors
    /// Returns an error if locking fails.
    pub fn lock(&self) -> std::io::Result<RegistrationGuard<'_>> {
        self.0.lock()?;
        Ok(RegistrationGuard(self))
    }
}

/// Exclusive access to registration publication and removal.
pub struct RegistrationGuard<'a>(&'a RegistrationLock);

impl Drop for RegistrationGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.0.unlock();
    }
}
