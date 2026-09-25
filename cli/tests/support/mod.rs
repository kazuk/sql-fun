use std::path::Path;

pub(super) struct CatalogProject {
    directory: tempfile::TempDir,
}

impl CatalogProject {
    pub(super) fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("Cargo.toml"), "[package.metadata.database.default]\nengine = \"PostgreSQL\"\ndialect = \"postgres\"\n").unwrap();
        Self { directory }
    }

    pub(super) fn arguments(&self, catalog: &Path) -> sql_fun_core::SqlFunArgs {
        sql_fun_core::SqlFunArgsBuilder::default()
            .metadata_file(self.directory.path().join("Cargo.toml").to_str().unwrap())
            .builtin_schema_dir(catalog.to_owned())
            .build()
            .unwrap()
    }
}
