use std::{
    fs::File,
    path::{Path, PathBuf},
};

use crate::{ServerError, pid_file::PidFile};

pub(crate) struct PidFileFixture {
    _directory: ::tempfile::TempDir,
    path: PathBuf,
}

impl PidFileFixture {
    pub(crate) fn new() -> Result<Self, std::io::Error> {
        let directory = ::tempfile::tempdir()?;
        let path = directory.path().join("primary/server.pid");
        Ok(Self {
            _directory: directory,
            path,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn register(&self, port: u16) -> Result<Option<PidFile>, ServerError> {
        PidFile::try_register_at_path(&self.path, port)
    }

    pub(crate) fn read(&self) -> Result<PidFile, ServerError> {
        Ok(::serde_json::from_slice(&std::fs::read(&self.path)?)?)
    }

    pub(crate) fn exists(&self) -> bool {
        self.path.exists()
    }

    pub(crate) fn names(&self) -> Result<Vec<PathBuf>, std::io::Error> {
        let parent = self.path.parent().expect("fixture parent");
        if !parent.exists() {
            return Ok(Vec::new());
        }
        std::fs::read_dir(parent)?
            .filter_map(|entry| match entry {
                Ok(entry) if entry.file_name() == "server.pid.lock" => None,
                Ok(entry) => Some(Ok(entry.path())),
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    pub(crate) fn open_lock_observer(&self) -> Result<File, std::io::Error> {
        File::options().read(true).write(true).open(&self.path)
    }

    pub(crate) fn persist_exited_primary(&self) -> Result<(), ServerError> {
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "test_helpers::pid_file::exited_primary_child",
                "--ignored",
            ])
            .spawn()?;
        let pid = child.id();
        assert!(child.wait()?.success());
        self.persist(
            &::serde_json::json!({ "pid": pid, "port": 30000, "token": "stale" }).to_string(),
        )?;
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn persist_dangling_symlink(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(self.path.parent().expect("fixture parent"))?;
        std::os::unix::fs::symlink("missing-registration", &self.path)
    }

    pub(crate) fn replace_with_malformed_primary(&self) -> Result<(), std::io::Error> {
        std::fs::remove_file(&self.path)?;
        self.persist_malformed_primary()
    }

    pub(crate) fn persist_malformed_primary(&self) -> Result<(), std::io::Error> {
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
