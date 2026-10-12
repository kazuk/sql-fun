use super::{SchemaCacheService, SchemaSelection};
use ::sql_fun_core::{Hasher, SqlFunArgs, SqlFunMetadata, SqlFunMetadataCollection};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Small synthetic builtin catalog: no external service/full builtin parsing.
pub(super) struct SchemaProject {
    directory: ::tempfile::TempDir,
}

impl SchemaProject {
    pub(super) fn new() -> Self {
        let directory = ::tempfile::tempdir().unwrap();
        let builtin = directory.path().join("postgres/17");
        std::fs::create_dir_all(&builtin).unwrap();
        for name in ["functions.json", "func_detail.json"] {
            std::fs::write(builtin.join(name), "[]").unwrap();
        }
        for name in ["left_unary_op.json", "right_unary_op.json", "casts.json"] {
            std::fs::write(builtin.join(name), "{}").unwrap();
        }
        std::fs::write(
            builtin.join("binary_op.json"),
            r#"{"+": [{"left":"int4", "right":"int4", "result":"int4", "strict":true}]}"#,
        )
        .unwrap();
        std::fs::write(builtin.join("pg_type.json"), r#"[{"name":"int4"}]"#).unwrap();
        std::fs::write(builtin.join("schema.sql"), "-- synthetic builtin\n").unwrap();
        std::fs::write(
            directory.path().join("Cargo.toml"),
            r"
[package.metadata.database.products]
engine = 'PostgreSQL'
dialect = 'postgres'
version = '17'
search_path = ['public']
connector = 'products-connector'
[package.metadata.database.orders]
engine = 'PostgreSQL'
dialect = 'postgres'
version = '17'
search_path = ['public']
connector = 'orders-connector'
",
        )
        .unwrap();
        Self { directory }
    }

    pub(super) fn home(&self) -> &Path {
        self.directory.path()
    }
    pub(super) fn service(&self) -> SchemaCacheService {
        SchemaCacheService::new(self.home()).unwrap()
    }
    pub(super) fn schema(&self, name: &str, sql: &str) -> PathBuf {
        let path = self.directory.path().join(name);
        std::fs::write(&path, sql).unwrap();
        path
    }
    pub(super) fn alias(&self, name: &str, target: &Path) -> PathBuf {
        let alias = self.home().join(name);
        std::os::unix::fs::symlink(target, &alias).unwrap();
        alias
    }
    pub(super) fn selection(
        &self,
        datasource: &str,
        extensions: &[PathBuf],
        dump: &Path,
        expected: &str,
    ) -> SchemaSelection {
        let (args, metadata) = self.analysis_inputs(datasource);
        SchemaSelection::new(datasource, &args, &metadata, extensions, dump, expected).unwrap()
    }
    pub(super) fn analysis_inputs(&self, datasource: &str) -> (SqlFunArgs, SqlFunMetadata) {
        let metadata_path = self.directory.path().join("Cargo.toml");
        let metadata = SqlFunMetadataCollection::load_from(&metadata_path).unwrap();
        let args = SqlFunArgs::builder(&HashMap::new())
            .unwrap()
            .sql_fun_home(self.home())
            .metadata_file(metadata_path.to_str().unwrap())
            .build()
            .unwrap();
        (args, metadata.get(datasource).unwrap().clone())
    }
    pub(super) fn builtin_sql(&self) -> PathBuf {
        self.home().join("postgres/17/schema.sql")
    }
    pub(super) fn builtin_key(&self) -> super::SchemaContextKey {
        super::SchemaContextKey::root(&"17".parse().unwrap(), &["public".to_owned()])
            .layer(
                &Self::hash(&self.builtin_sql()),
                &self.builtin_sql().canonicalize().unwrap(),
            )
            .unwrap()
    }
    pub(super) fn hash(path: &Path) -> String {
        Hasher::hash_file(path).unwrap()
    }
}
