use super::{SchemaCacheService, SchemaSelection};
use ::sql_fun_core::{Hasher, SqlFunArgs, SqlFunMetadata, SqlFunMetadataCollection};
use std::{
    collections::HashMap,
    fmt::Write,
    path::{Path, PathBuf},
};

/// Small synthetic builtin catalog: no external service/full builtin parsing.
pub(crate) struct SchemaProject {
    directory: ::tempfile::TempDir,
}

impl SchemaProject {
    /// Initialize a synthetic table without a `PostgreSQL` server or full builtin parsing.
    pub(crate) async fn initialized_schema(
        &self,
        service: &SchemaCacheService,
    ) -> std::sync::Arc<super::InitializedSchema> {
        let dump = self.schema(
            "registry.sql",
            "CREATE TABLE retained (id int4, value int4);",
        );
        let selection = self.selection("products", &[], &dump, &Self::hash(&dump));
        std::sync::Arc::new(service.initialize(&selection).await.unwrap())
    }

    pub(crate) fn new() -> Self {
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
        std::fs::write(
            builtin.join("collations.json"),
            r#"{
                "database": {
                    "encoding": 6, "encoding_name": "UTF8", "provider": "c",
                    "collate": "C", "ctype": "C", "locale": null,
                    "icu_rules": null, "version": null,
                    "default_collation": {"schema": "pg_catalog", "name": "default", "encoding": -1}
                },
                "collations": [{
                    "schema": "pg_catalog", "name": "default", "encoding": -1,
                    "provider": "d", "deterministic": true,
                    "collate": null, "ctype": null, "locale": null,
                    "icu_rules": null, "version": null
                }]
            }"#,
        )
        .unwrap();
        std::fs::write(builtin.join("operator_classes.json"), r#"[{"schema":"pg_catalog","name":"int4_ops","access_method":"btree",
                "family":{"schema":"pg_catalog","name":"integer_ops","access_method":"btree"},
                "input_type":{"schema":"pg_catalog","name":"int4"},"default":true,"key_type":null}]"#).unwrap();
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

    pub(crate) fn rpc_request(&self, sql: &str) -> ::serde_json::Value {
        let dump = self.schema("rpc.sql", sql);
        // Exercise the same core-owned transfer as a CLI-selected datasource.
        let mut builder = SqlFunArgs::builder(&HashMap::new()).unwrap();
        builder
            .sql_fun_home(self.home())
            .metadata_file(self.home().join("Cargo.toml").to_str().unwrap())
            .datasource(Some("products".to_owned()))
            .schema_file(&dump);
        let args = builder.build().unwrap();
        ::serde_json::json!({
            "datasource":"products", "schema_dump":dump,
            "expected_hash":Self::hash(&dump), "environments":args.environments().unwrap()
        })
    }

    pub(crate) fn request_with_extensions(&self) -> ::serde_json::Value {
        let request = self.rpc_request("DROP TABLE missing_app;");
        let (args, metadata) = self.analysis_inputs("products");
        let directory = args.postgres_extensions_dir(&metadata).unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("first--1.0.sql"),
            "DROP TABLE missing_first;",
        )
        .unwrap();
        std::fs::write(
            directory.join("second--2.0.sql"),
            "DROP TABLE missing_second;",
        )
        .unwrap();
        let mut request = request;
        request["environments"]["SQL_FUN_EXTENSIONS"] = ::serde_json::json!("second@2.0,first@1.0");
        request
    }

    pub(crate) fn diagnostic_request(&self, count: usize) -> ::serde_json::Value {
        let mut sql = String::new();
        for index in 0..count {
            ::std::writeln!(&mut sql, "DROP TABLE missing_{index};").unwrap();
        }
        self.rpc_request(&sql)
    }

    pub(crate) fn home(&self) -> &Path {
        self.directory.path()
    }
    pub(crate) fn service(&self) -> SchemaCacheService {
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
    pub(super) fn schema_environment(&self) -> (SqlFunArgs, SqlFunMetadata) {
        super::SchemaCacheService::schema_environment(
            self.home(),
            &"17".parse().unwrap(),
            &["public".to_owned()],
            None,
        )
        .unwrap()
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
        super::SchemaContextKey::root(&"17".parse().unwrap(), &["public".to_owned()], None)
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

/// A synchronized synthetic root load, used to test cancellation at the RPC
/// boundary without delays or a full builtin catalog.
pub(crate) struct SuspendedRoot {
    api: std::sync::Arc<crate::Api>,
    home: PathBuf,
    key: super::SchemaContextKey,
    resume: ::tokio::sync::oneshot::Sender<()>,
    task: ::tokio::task::JoinHandle<()>,
}

impl SchemaProject {
    pub(crate) async fn suspend_root(&self, api: &std::sync::Arc<crate::Api>) -> SuspendedRoot {
        let home = self.home().to_path_buf();
        let version = "17".parse().unwrap();
        let search_path = vec!["public".to_owned()];
        let key = super::SchemaContextKey::root(&version, &search_path, None);
        let root =
            SchemaCacheService::load_root(&home, &version, &search_path, None, &key).unwrap();
        let (resume, resumed) = ::tokio::sync::oneshot::channel();
        let (ready, started) = ::tokio::sync::oneshot::channel();
        let task_api = std::sync::Arc::clone(api);
        let task_home = home.clone();
        let task_key = key.clone();
        let task = ::tokio::spawn(async move {
            let service = task_api.schema_cache_service(&task_home).unwrap();
            service
                .roots
                .load(&task_key, move || async move {
                    ready.send(()).unwrap();
                    resumed.await.unwrap();
                    Ok(root)
                })
                .await
                .unwrap();
        });
        started.await.unwrap();
        SuspendedRoot {
            api: std::sync::Arc::clone(api),
            home,
            key,
            resume,
            task,
        }
    }
}

impl SuspendedRoot {
    pub(crate) async fn wait_for_rpc(&self) {
        ::tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while self
                .api
                .schema_cache_service(&self.home)
                .unwrap()
                .roots
                .active_waiters(&self.key)
                < 2
            {
                ::tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    pub(crate) async fn complete(self) {
        self.resume.send(()).unwrap();
        self.task.await.unwrap();
        let roots = &self.api.schema_cache_service(&self.home).unwrap().roots;
        assert!(roots.lookup(&self.key).is_some());
    }
}

impl SchemaProject {
    pub(crate) fn analysis_context_over(
        &self,
        base: &::sql_fun_sqlast::sem::FrozenBaseContext,
        datasource: &str,
    ) -> ::sql_fun_sqlast::sem::SchemaFileContext {
        let (args, metadata) = self.analysis_inputs(datasource);
        ::sql_fun_sqlast::sem::SchemaFileContext::new(
            &self.home().join("query.sql"),
            base,
            &args,
            &metadata,
        )
    }

    /// Analyze against this fixture's small synthetic catalog; no full builtin
    /// schema parsing or external service is involved.
    pub(crate) fn analysis_context(
        &self,
        datasource: &str,
    ) -> ::sql_fun_sqlast::sem::SchemaFileContext {
        let (args, metadata) = self.analysis_inputs(datasource);
        let base = ::sql_fun_sqlast::sem::BaseContext::new(&args, &metadata).unwrap();
        ::sql_fun_sqlast::sem::SchemaFileContext::new(
            &self.home().join("query.sql"),
            &::sql_fun_sqlast::sem::FrozenBaseContext::from(base),
            &args,
            &metadata,
        )
    }
}

impl SchemaProject {
    pub(super) fn configure_roles(&self, datasource: &str, declaration: &str) {
        let path = self.home().join("Cargo.toml");
        let source = std::fs::read_to_string(&path).unwrap();
        let header = format!("[package.metadata.database.{datasource}]\n");
        let start = source.find(&header).unwrap() + header.len();
        // Role arrays contain '[' too; find the next table header by its line prefix.
        let end = source[start..]
            .find("\n[")
            .map_or(source.len(), |offset| start + offset + 1);
        let body = source[start..end]
            .lines()
            .filter(|line| !line.starts_with("roles ="))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(
            path,
            format!(
                "{}{}\n{}\n{}",
                &source[..start],
                body,
                declaration,
                &source[end..]
            ),
        )
        .unwrap();
    }

    pub(super) fn apply_sql(
        context: ::sql_fun_sqlast::sem::SchemaFileContext,
        sql: &str,
    ) -> ::sql_fun_sqlast::AstAndContextPair<::sql_fun_sqlast::sem::SchemaFileContext> {
        use ::sql_fun_sqlast::sem::{MutSourcePathWithSpan, SemAst, SourcePathWithSpan};
        let mut context = context;
        context.set_source_path_with_span(&SourcePathWithSpan::new(
            Path::new("roles.sql"),
            &::sql_fun_sqlast::StringSpan::new(0, sql.len()),
        ));
        let syntax = ::sql_fun_sqlast::PostgresDialect::parse(sql).unwrap();
        let tokens = ::sql_fun_sqlast::PostgresDialect::scan(sql).unwrap();
        SemAst::semantic_analysis(context, syntax, &tokens).unwrap()
    }
}
