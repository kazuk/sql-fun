use std::fmt::Write;

/// A small synthetic catalog for real-server diagnostic paging tests.
pub struct SchemaFixture {
    directory: ::tempfile::TempDir,
    request: ::serde_json::Value,
}
impl SchemaFixture {
    pub const fn request(&self) -> &::serde_json::Value {
        &self.request
    }

    /// Writes small fixture-owned files; no catalog parsing or external service.
    pub fn new() -> ::testresult::TestResult<Self> {
        let mut sql = String::new();
        for index in 0..513 {
            ::std::writeln!(sql, "DROP TABLE missing_{index};")?;
        }
        Self::with_sql(&sql)
    }

    /// A minimal catalog with explicitly supplied schema SQL.
    pub fn with_sql(sql: &str) -> ::testresult::TestResult<Self> {
        let directory = ::tempfile::tempdir()?;
        let root = directory.path();
        let catalog = root.join("home/postgres/17");
        std::fs::create_dir_all(&catalog)?;
        for name in ["functions", "func_detail", "pg_type"] {
            std::fs::write(catalog.join(format!("{name}.json")), "[]")?;
        }
        std::fs::write(
            catalog.join("operator_classes.json"),
            r#"[{"schema":"pg_catalog","name":"int4_ops","access_method":"btree",
                "family":{"schema":"pg_catalog","name":"integer_ops","access_method":"btree"},
                "input_type":{"schema":"pg_catalog","name":"int4"},"default":true,"key_type":null}]"#,
        )?;
        for name in ["left_unary_op", "right_unary_op", "casts"] {
            std::fs::write(catalog.join(format!("{name}.json")), "{}")?;
        }
        std::fs::write(
            catalog.join("binary_op.json"),
            r#"{"intentionally_unused":[]}"#,
        )?;
        std::fs::write(
            catalog.join("collations.json"),
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
        )?;
        std::fs::write(catalog.join("schema.sql"), "-- synthetic builtin")?;
        std::fs::write(
            root.join("Cargo.toml"),
            r#"
[package.metadata.database.products]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
search_path = ["public"]
connector = "products-connector"
"#,
        )?;
        let dump = root.join("schema.sql");
        std::fs::write(&dump, sql)?;
        let request = ::serde_json::json!({
            "datasource":"products", "schema_dump":dump,
            "expected_hash": ::sha256::digest(sql),
            "environments": {"SQL_FUN_HOME":root.join("home"), "SQL_FUN_METADATA_FILE":root.join("Cargo.toml")}
        });
        Ok(Self { directory, request })
    }
}

impl SchemaFixture {
    /// A lightweight query catalog with the SQL types used by bridge analysis tests.
    #[allow(
        dead_code,
        reason = "shared with CLI analysis tests; client initialization tests do not call this constructor"
    )]
    pub fn with_query_schema(sql: &str) -> ::testresult::TestResult<Self> {
        let fixture = Self::with_sql(sql)?;
        std::fs::write(
            fixture
                .directory
                .path()
                .join("home/postgres/17/pg_type.json"),
            r#"[{"name":"int4"},{"name":"text"},{"name":"bool"}]"#,
        )?;
        Ok(fixture)
    }
}
