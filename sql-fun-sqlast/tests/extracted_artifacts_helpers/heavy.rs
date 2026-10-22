use ::sql_fun_sqlast::{
    PostgresDialect,
    sem::{BaseContext, BaseParseContext, FullName, TableName},
};

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

/// Copies a real producer generation and parses its builtin/user schemas per test.
/// No initialized catalog is shared between tests.
pub(crate) struct InitializedCatalog {
    directory: ::tempfile::TempDir,
}
impl InitializedCatalog {
    pub(crate) fn new(version: &str) -> Self {
        let fixture = Self {
            directory: ::tempfile::tempdir().unwrap(),
        };
        fixture.install_fixture(version);
        fixture
    }
    fn home(&self) -> std::path::PathBuf {
        self.directory.path().join("home")
    }
    fn install_fixture(&self, version: &str) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("sql-fun-home/postgres")
            .join(version);
        let target = self.home().join("postgres").join(version);
        std::fs::create_dir_all(&target).unwrap();
        for name in [
            "metadata.json",
            "schema.sql",
            "pg_type.json",
            "functions.json",
            "func_detail.json",
            "binary_op.json",
            "left_unary_op.json",
            "right_unary_op.json",
            "casts.json",
            "collations.json",
            "operator_classes.json",
        ] {
            std::fs::copy(source.join(name), target.join(name)).unwrap();
        }
    }
    pub(crate) fn assert_semantics(&self, version: &str) {
        let metadata = self.directory.path().join("Cargo.toml");
        std::fs::write(&metadata, format!("[package.metadata.database.default]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '{version}'\n")).unwrap();
        let args = ::sql_fun_core::SqlFunArgsBuilder::default()
            .metadata_file(metadata.to_str().unwrap())
            .sql_fun_home(self.home())
            .build()
            .unwrap();
        let metadata = args.load_metadata().unwrap();
        let context = BaseContext::new(&args, metadata.get("default").unwrap()).unwrap();
        let builtins = {
            let analysis_base = &::sql_fun_sqlast::sem::FrozenBaseContext::from(context);
            PostgresDialect::parse_schema_file(
                &self.home().join(format!("postgres/{version}/schema.sql")),
                analysis_base,
                &mut None,
                &args,
                metadata.get("default").unwrap(),
            )
        }
        .unwrap();
        let pg_class = TableName::from(FullName::with_schema("pg_catalog", "pg_class"));
        assert!(builtins.get_table(&pg_class).is_some());
        let user = self.directory.path().join("schema.develop.sql");
        let sql =
            "CREATE TABLE hoge (id int PRIMARY KEY); SELECT abs(id) + 1 AS next_id FROM hoge;";
        std::fs::write(&user, sql).unwrap();
        let parsed = {
            let analysis_base = builtins.base_context();
            PostgresDialect::parse_schema_file(
                &user,
                analysis_base,
                &mut None,
                &args,
                metadata.get("default").unwrap(),
            )
        }
        .unwrap();
        assert!(
            parsed
                .get_table(&TableName::from(FullName::with_schema("public", "hoge")))
                .is_some()
        );
        assert_eq!(std::fs::read_to_string(user).unwrap(), sql);
    }
}
