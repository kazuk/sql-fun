use std::path::Path;

/// Fixture for loading the real catalog. `BaseContext` initialization in its callers
/// parses builtin metadata per test and is intentionally kept in heavy setup.
pub(crate) struct CatalogProject {
    directory: ::tempfile::TempDir,
}

impl CatalogProject {
    pub(crate) fn new() -> Self {
        let directory = ::tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("Cargo.toml"), "[package.metadata.database.default]\nengine = \"PostgreSQL\"\ndialect = \"postgres\"\n").unwrap();
        Self { directory }
    }

    pub(crate) fn arguments(&self, catalog: &Path) -> ::sql_fun_core::SqlFunArgs {
        ::sql_fun_core::SqlFunArgsBuilder::default()
            .metadata_file(self.directory.path().join("Cargo.toml").to_str().unwrap())
            .builtin_schema_dir(catalog.to_owned())
            .build()
            .unwrap()
    }
}
