use std::path::Path;

pub(super) struct BundleFixture {
    root: tempfile::TempDir,
}

impl BundleFixture {
    pub(super) fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        super::catalog::write_json(
            &root.path().join("metadata.json"),
            &serde_json::json!({"format_version": 1, "server_version_num": 170_009}),
        )
        .unwrap();
        Self { root }
    }

    pub(super) fn path(&self) -> &Path {
        self.root.path()
    }

    pub(super) fn import_entries(
        &self,
        entries: &[(&str, &[u8], u8)],
    ) -> Result<(), anyhow::Error> {
        let bundle = self.path().join("input.tar");
        let mut archive = tar::Builder::new(std::fs::File::create(&bundle)?);
        for (path, bytes, kind) in entries {
            let mut header = tar::Header::new_gnu();
            // Raw header writing intentionally permits adversarial names for import tests.
            header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::new(*kind));
            header.set_cksum();
            archive.append(&header, *bytes)?;
        }
        archive.finish()?;
        super::bundle::import(&bundle, self.path())
    }
}

pub(super) struct PublicationFixture {
    root: tempfile::TempDir,
}

impl PublicationFixture {
    pub(super) fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }

    pub(super) fn prepare(
        &self,
        revision: &str,
    ) -> Result<super::publication::Publication, anyhow::Error> {
        let stage = super::publication::Publication::new(&self.root.path().join("catalog"))?;
        std::fs::write(stage.path().join(revision), revision)?;
        Ok(stage)
    }

    pub(super) fn has_revision(&self, revision: &str) -> bool {
        self.root.path().join("catalog").join(revision).is_file()
    }
}
