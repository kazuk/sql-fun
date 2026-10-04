use super::PidFile;
use crate::ServerError;

use std::fs::File;
use std::path::PathBuf;

pub(super) struct PidFileFixture {
    _directory: ::tempfile::TempDir,
    path: PathBuf,
}

impl PidFileFixture {
    pub(super) fn new() -> Result<Self, std::io::Error> {
        let directory = ::tempfile::tempdir()?;
        let path = directory.path().join("primary/server.pid");
        Ok(Self {
            _directory: directory,
            path,
        })
    }

    pub(super) fn register(&self, port: u16) -> Result<Option<PidFile>, ServerError> {
        PidFile::try_register_at_path(&self.path, port)
    }

    pub(super) fn read(&self) -> Result<PidFile, ServerError> {
        PidFile::from_file(&self.path)
    }

    pub(super) fn exists(&self) -> bool {
        self.path.exists()
    }

    pub(super) fn open_lock_observer(&self) -> Result<File, std::io::Error> {
        File::options().read(true).write(true).open(&self.path)
    }

    pub(super) fn persist_exited_primary(&self) -> Result<PidFile, ServerError> {
        // Reuse the test executable rather than requiring a shell or another program.
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "pid_file::test_helpers::exited_primary_child",
                "--ignored",
            ])
            .spawn()?;
        let pid = child.id();
        assert!(child.wait()?.success());
        let mut stale = PidFile::new(30000);
        stale.pid = pid;
        self.persist(&::serde_json::to_string(&stale)?)?;
        Ok(stale)
    }

    pub(super) fn persist_malformed_primary(&self) -> Result<(), std::io::Error> {
        self.persist("invalid PID JSON")
    }

    fn persist(&self, content: &str) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(self.path.parent().expect("fixture parent"))?;
        std::fs::write(&self.path, content)
    }
}

#[test]
#[ignore = "invoked in a child process to obtain an exited PID"]
fn exited_primary_child() {}
