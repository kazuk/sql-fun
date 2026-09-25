use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

use ::anyhow::Context;

pub(super) struct Publication {
    stage: tempfile::TempDir,
    destination: PathBuf,
    _lock: File,
}

impl Publication {
    pub(super) fn new(destination: &Path) -> Result<Self, anyhow::Error> {
        let name = destination
            .file_name()
            .context("output must name a directory")?;
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let parent = fs::canonicalize(parent)?;
        let destination = parent.join(name);
        let mut lock_name = name.to_os_string();
        lock_name.push(".sql-fun.lock");
        let lock = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(parent.join(lock_name))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .context("another extraction/import owns this output")?;
        if let Ok(metadata) = fs::symlink_metadata(&destination) {
            anyhow::ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "output must be a real directory, not a symlink or file"
            );
        }
        let stage = tempfile::Builder::new()
            .prefix(".sql-fun-stage-")
            .tempdir_in(parent)?;
        Ok(Self {
            stage,
            destination,
            _lock: lock,
        })
    }

    pub(super) fn path(&self) -> &Path {
        self.stage.path()
    }

    pub(super) fn copy_catalog(&self) -> Result<(), anyhow::Error> {
        for entry in fs::read_dir(&self.destination)
            .context("extract catalogs before importing extensions")?
        {
            let entry = entry?;
            if entry.file_name() == "extension" {
                continue;
            }
            anyhow::ensure!(
                entry.file_type()?.is_file(),
                "catalog contains an unexpected non-file entry"
            );
            fs::copy(entry.path(), self.path().join(entry.file_name()))?;
        }
        Ok(())
    }

    pub(super) fn publish(self) -> Result<(), anyhow::Error> {
        for entry in walkdir::WalkDir::new(self.path()).contents_first(true) {
            File::open(entry?.path())?.sync_all()?;
        }
        if self.destination.exists() {
            #[cfg(target_os = "linux")]
            rustix::fs::renameat_with(
                rustix::fs::CWD,
                self.path(),
                rustix::fs::CWD,
                &self.destination,
                rustix::fs::RenameFlags::EXCHANGE,
            )
            .context(
                "atomic output replacement requires rename-exchange support on this filesystem",
            )?;
            #[cfg(not(target_os = "linux"))]
            anyhow::bail!(
                "replacing existing output atomically currently requires Linux; choose a new output directory"
            );
        } else {
            fs::rename(self.path(), &self.destination)?;
        }
        File::open(self.destination.parent().context("output parent")?)?.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::PublicationFixture;

    #[test]
    fn failure_preserves_old_output_and_success_replaces_whole_generation() {
        let fixture = PublicationFixture::new();
        fixture.prepare("valid").unwrap().publish().unwrap();
        {
            let _aborted = fixture.prepare("incomplete").unwrap();
            assert!(fixture.prepare("concurrent").is_err());
        }
        assert!(fixture.has_revision("valid"));
        assert!(!fixture.has_revision("incomplete"));
        fixture.prepare("complete").unwrap().publish().unwrap();
        assert!(!fixture.has_revision("valid"));
        assert!(fixture.has_revision("complete"));
    }
}
