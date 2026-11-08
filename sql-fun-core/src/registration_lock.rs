//! Coordination of publication and removal of a per-user server registration.
//!
//! The persistent sibling `.lock` inode serializes every mutation of the
//! registration name. Publication still links a complete, shared-locked inode;
//! the owner keeps that lifetime lock until exit. A stale remover acquires the
//! old inode exclusively *before* coordination, then rechecks its pathname.
//! Publication, rollback, owner drop, and client recovery hold coordination
//! while checking and mutating the name. Thus a checked name cannot be replaced
//! by another participant between the check and unlink or signal.
//!
//! Never wait for a lifetime lock, network readiness, or process exit while
//! holding coordination: the exiting owner may need it. Owner drop only tries
//! coordination; contention leaves a stale registration rather than blocking
//! shutdown. Crashes release both kernel locks; a leftover coordination inode
//! is reusable and must never be routinely unlinked. An open observed
//! registration prevents inode reuse while its identity is being compared.
//! These guarantees cover cooperating current-protocol clients and servers,
//! not arbitrary actors that ignore advisory locks.

use std::{
    fs::{File, OpenOptions},
    path::Path,
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// A persistent coordination inode for a registration path.
///
/// All publishers and removers must hold its exclusive guard while changing the
/// registration name. The coordination file must not be removed while in use. Locks coordinate
/// cooperating participants; they do not prevent arbitrary filesystem mutation.
/// Each operation must open its own lock; do not share one open description
/// across simultaneous operations (file locks are not thread mutexes).
pub struct RegistrationLock(File);

impl RegistrationLock {
    /// Open the coordination file in the registration's existing directory.
    ///
    /// # Errors
    /// Returns an error if the coordination file cannot be opened. Existing
    /// directory and file permissions are neither restricted nor changed.
    pub fn open(registration: &Path) -> std::io::Result<Self> {
        let parent = registration.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "registration has no parent",
            )
        })?;
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
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        options.mode(0o600);
        let file = options.open(&path)?;
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

    /// Acquire the coordination lock with a bounded wait for another participant.
    ///
    /// # Errors
    /// Returns an error if locking fails or the two-second wait expires.
    pub fn lock(&self) -> std::io::Result<RegistrationGuard<'_>> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match self.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return Err(error);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        }
    }
}

/// Exclusive access to registration publication and removal.
pub struct RegistrationGuard<'a>(&'a RegistrationLock);

impl Drop for RegistrationGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.0.unlock();
    }
}
