//! Integration coverage for loading the committed PostgreSQL builtin schema.

use std::path::PathBuf;

use ::clap::Parser;
use ::sql_fun_sqlast::{
    PostgresDialect, SqlFunArgs,
    sem::{BaseContext, BaseParseContext, FullName, TableName},
};

#[test]
fn parses_the_committed_builtin_schemas() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let home = PathBuf::from(env!("SQL_FUN_HOME"));
    let metadata = manifest.join("sql_fun.metadata.toml");
    for version in ["17", "18"] {
        let args = SqlFunArgs::try_parse_from([
            "sqlfun",
            "--metadata-file",
            metadata.to_str().expect("metadata path is UTF-8"),
            "--sql-fun-home",
            home.to_str().expect("builtin path is UTF-8"),
            "--postgres-version",
            version,
        ])?;

        let context = BaseContext::new(
            &args,
            args.load_metadata()?
                .get("default")
                .expect("selected test metadata"),
        )?;
        let schema = home.join(format!("postgres/{version}/schema.sql"));
        let parsed = PostgresDialect::parse_schema_file(&schema, context)?;
        let pg_class = TableName::from(FullName::with_schema("pg_catalog", "pg_class"));
        assert!(
            parsed.get_table(&pg_class).is_some(),
            "PostgreSQL {version}"
        );
    }
    Ok(())
}
