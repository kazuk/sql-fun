use crate::connection::{ConnectionError, Registration};
use ::rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fd::OwnedFd,
    io::Errno,
    process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal},
};
use ::sql_fun_core::{RegistrationGuard, RegistrationLock};
use ::tokio::time::{Instant, sleep_until};
use std::{os::unix::fs::MetadataExt, path::Path, time::Duration};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

fn failure(last: &str, detail: &str) -> ConnectionError {
    ConnectionError::failure(
        "recovery",
        &format!("{detail}; last readiness failure: {last}"),
    )
}

async fn lock_until<'a>(
    lock: &'a RegistrationLock,
    deadline: Instant,
    last: &str,
) -> Result<RegistrationGuard<'a>, ConnectionError> {
    loop {
        if Instant::now() >= deadline {
            return Err(failure(
                last,
                "deadline expired waiting for registration coordination",
            ));
        }
        match lock.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                sleep_until(deadline.min(Instant::now() + POLL_INTERVAL)).await;
            }
            Err(error) => {
                return Err(failure(
                    last,
                    &format!("cannot coordinate registration: {error}"),
                ));
            }
        }
    }
}

struct Process {
    pid: u32,
    handle: OwnedFd,
}

impl Process {
    fn open(pid: u32) -> Result<Option<Self>, Errno> {
        let raw = i32::try_from(pid).map_err(|_| Errno::INVAL)?;
        let pid_arg = Pid::from_raw(raw).ok_or(Errno::INVAL)?;
        match pidfd_open(pid_arg, PidfdFlags::empty()) {
            Ok(handle) => Ok(Some(Self { pid, handle })),
            Err(Errno::SRCH) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn exited(&self) -> Result<bool, Errno> {
        let mut descriptor = [PollFd::new(&self.handle, PollFlags::IN)];
        poll(&mut descriptor, Some(&Timespec::default()))?;
        let events = descriptor[0].revents();
        if events.intersects(PollFlags::ERR | PollFlags::NVAL) {
            return Err(Errno::IO);
        }
        Ok(events.intersects(PollFlags::IN | PollFlags::HUP))
    }

    // None means the process exited. Checking the pidfd after the /proc lookup
    // binds the observation to this still-live process, even if its numeric PID
    // was recycled during inspection. Executable mutation is unsupported.
    fn matches_executable(&self, expected: &std::fs::Metadata) -> std::io::Result<Option<bool>> {
        if self.exited()? {
            return Ok(None);
        }
        let actual = std::fs::metadata(format!("/proc/{}/exe", self.pid));
        if self.exited()? {
            return Ok(None);
        }
        let actual = actual?;
        Ok(Some(
            expected.dev() == actual.dev() && expected.ino() == actual.ino(),
        ))
    }

    fn signal(&self, signal: Signal) -> Result<(), Errno> {
        pidfd_send_signal(&self.handle, signal)
    }

    async fn wait_until(&self, deadline: Instant) -> Result<bool, Errno> {
        loop {
            if self.exited()? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            sleep_until(deadline.min(Instant::now() + POLL_INTERVAL)).await;
        }
    }
}

struct Recovery<'a> {
    lock: RegistrationLock,
    path: &'a Path,
    registration: &'a Registration,
    executable: &'a Path,
    deadline: Instant,
    last: &'a str,
}

impl Recovery<'_> {
    fn current(&self) -> Result<bool, ConnectionError> {
        self.registration
            .still_at(self.path)
            .map_err(|error| failure(self.last, &format!("cannot recheck registration: {error}")))
    }

    async fn remove_observed(&self, deadline: Instant) -> Result<(), ConnectionError> {
        let _guard = lock_until(&self.lock, deadline, self.last).await?;
        self.remove_current()
    }

    // Caller holds mutation coordination through both the check and unlink.
    fn remove_current(&self) -> Result<(), ConnectionError> {
        if self.current()? {
            std::fs::remove_file(self.path).map_err(|error| {
                failure(
                    self.last,
                    &format!("cannot remove stale registration: {error}"),
                )
            })?;
        }
        Ok(())
    }

    fn verification_failure(&self, error: &std::io::Error) -> ConnectionError {
        failure(
            self.last,
            &format!(
                "cannot verify registered PID {} against {}: {error}; registration preserved",
                self.registration.pid(),
                self.executable.display()
            ),
        )
    }

    async fn signal_if_current(
        &self,
        process: &Process,
        signal: Signal,
        deadline: Instant,
    ) -> Result<bool, ConnectionError> {
        loop {
            let error = {
                let _guard = lock_until(&self.lock, deadline, self.last).await?;
                if !self.current()? {
                    return Ok(false);
                }
                let expected = std::fs::metadata(self.executable)
                    .map_err(|error| self.verification_failure(&error))?;
                match process.matches_executable(&expected) {
                    Ok(None) => return Ok(true),
                    Ok(Some(false)) => {
                        self.remove_current()?;
                        return Ok(false);
                    }
                    Ok(Some(true)) => {
                        if Instant::now() >= deadline {
                            return Err(failure(
                                self.last,
                                "insufficient time to signal server safely",
                            ));
                        }
                        return match process.signal(signal) {
                            Ok(()) | Err(Errno::SRCH) => Ok(true),
                            Err(error) => Err(failure(
                                self.last,
                                &format!("cannot signal verified server: {error}"),
                            )),
                        };
                    }
                    Err(error) => {
                        if error.kind() != std::io::ErrorKind::NotFound {
                            return Err(self.verification_failure(&error));
                        }
                        error
                    }
                }
            };
            // /proc/<pid>/exe can disappear before the pidfd becomes readable
            // when another recoverer is terminating this process. Release the
            // coordination lock and recheck both the registration and pidfd.
            if Instant::now() >= deadline {
                return Err(self.verification_failure(&error));
            }
            sleep_until(deadline.min(Instant::now() + POLL_INTERVAL)).await;
        }
    }

    async fn terminate(&self, process: &Process) -> Result<(), ConnectionError> {
        let started = Instant::now();
        let remaining = self.deadline.saturating_duration_since(started);
        if remaining < Duration::from_millis(30) {
            return Err(failure(
                self.last,
                "insufficient time for termination and restart; registration preserved",
            ));
        }
        let termination_deadline = started + remaining / 2;
        let grace_deadline = started + remaining / 4;
        if !self
            .signal_if_current(process, Signal::TERM, termination_deadline)
            .await?
        {
            return Ok(());
        }
        if !process
            .wait_until(grace_deadline)
            .await
            .map_err(|error| failure(self.last, &format!("cannot wait for server exit: {error}")))?
        {
            if !self
                .signal_if_current(process, Signal::KILL, termination_deadline)
                .await?
            {
                return Ok(());
            }
            if !process
                .wait_until(termination_deadline)
                .await
                .map_err(|error| {
                    failure(self.last, &format!("cannot verify server exit: {error}"))
                })?
            {
                return Err(failure(
                    self.last,
                    "server did not exit after SIGKILL; registration preserved",
                ));
            }
        }
        self.remove_observed(termination_deadline).await
    }

    async fn run(&self) -> Result<(), ConnectionError> {
        let started = Instant::now();
        let cleanup_deadline = started + self.deadline.saturating_duration_since(started) / 2;
        {
            let _guard = lock_until(&self.lock, cleanup_deadline, self.last).await?;
            if !self.current()? {
                return Ok(());
            }
        }
        let process = Process::open(self.registration.pid()).map_err(|error| {
            failure(
                self.last,
                &format!(
                    "cannot safely open registered PID {}: {error}",
                    self.registration.pid()
                ),
            )
        })?;
        let Some(process) = process else {
            return self.remove_observed(cleanup_deadline).await;
        };
        if process.exited().map_err(|error| {
            failure(
                self.last,
                &format!("cannot observe registered process: {error}"),
            )
        })? {
            return self.remove_observed(cleanup_deadline).await;
        }
        self.terminate(&process).await
    }
}

pub(crate) async fn recover(
    path: &Path,
    registration: &Registration,
    executable: &Path,
    deadline: Instant,
    last: &str,
) -> Result<(), ConnectionError> {
    let lock = RegistrationLock::open(path).map_err(|error| {
        failure(
            last,
            &format!("cannot open registration coordination: {error}"),
        )
    })?;
    let outcome = Recovery {
        lock,
        path,
        registration,
        executable,
        deadline,
        last,
    }
    .run()
    .await;
    match outcome {
        Err(error) if Instant::now() >= deadline => Err(ConnectionError::timeout(
            path,
            Some(registration.pid()),
            &error.to_string(),
        )),
        result => result,
    }
}
