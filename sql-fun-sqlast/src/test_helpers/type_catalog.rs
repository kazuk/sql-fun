use std::path::Path;

pub(crate) struct TypeCatalog {
    _directory: tempfile::TempDir,
    path: std::path::PathBuf,
}

impl TypeCatalog {
    pub(crate) fn new(json: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("pg_type.json"), json).unwrap();
        let path = directory.path().join("pg_type.json");
        Self {
            _directory: directory,
            path,
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}
