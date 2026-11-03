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
        lock.try_lock()
            .context("another extraction/import owns this output")?;
        validate_empty(&destination)?;
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

    pub(super) fn copy_catalog(&self, source: &Path) -> Result<(), anyhow::Error> {
        for entry in fs::read_dir(source).context("extract catalogs before importing extensions")? {
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
        validate_empty(&self.destination)?;
        if !self.destination.exists() {
            fs::create_dir(&self.destination)?;
        }
        // Never replace a destination entry, including one created after validation.
        // A publication I/O failure can leave a partial candidate for inspection.
        for entry in walkdir::WalkDir::new(self.path()).min_depth(1) {
            let entry = entry?;
            let destination = self
                .destination
                .join(entry.path().strip_prefix(self.path())?);
            if entry.file_type().is_dir() {
                fs::create_dir(destination)?;
            } else {
                let mut target = File::options()
                    .write(true)
                    .create_new(true)
                    .open(destination)?;
                std::io::copy(&mut File::open(entry.path())?, &mut target)?;
                target.sync_all()?;
            }
        }
        File::open(&self.destination)?.sync_all()?;
        File::open(self.destination.parent().context("output parent")?)?.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::extract::test_helpers::PublicationFixture;

    #[test]
    fn refuses_nonempty_output_without_replacing_content() {
        let fixture = PublicationFixture::new();
        fixture.prepare("valid").unwrap().publish().unwrap();
        assert!(fixture.prepare("replacement").is_err());
        assert!(fixture.has_revision("valid"));
        assert!(!fixture.has_revision("replacement"));
    }

    #[test]
    fn accepts_empty_output_and_rejects_concurrent_writer() {
        let fixture = PublicationFixture::new();
        fixture.create_empty_output();
        let staged = fixture.prepare("complete").unwrap();
        assert!(fixture.prepare("concurrent").is_err());
        staged.publish().unwrap();
        assert!(fixture.has_revision("complete"));
    }

    #[test]
    fn rejects_content_added_before_publication() {
        let fixture = PublicationFixture::new();
        let staged = fixture.prepare("candidate").unwrap();
        fixture.add_existing_revision("foreign");
        assert!(staged.publish().is_err());
        assert!(fixture.has_revision("foreign"));
        assert!(!fixture.has_revision("candidate"));
    }

    #[test]
    fn aborted_staging_leaves_output_available() {
        let fixture = PublicationFixture::new();
        drop(fixture.prepare("incomplete").unwrap());
        fixture.prepare("complete").unwrap().publish().unwrap();
        assert!(!fixture.has_revision("incomplete"));
        assert!(fixture.has_revision("complete"));
    }
}

fn validate_empty(destination: &Path) -> Result<(), anyhow::Error> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "output must be a real directory, not a symlink or file"
            );
            anyhow::ensure!(
                fs::read_dir(destination)?.next().is_none(),
                "output must be absent or empty; existing contents will not be replaced"
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
