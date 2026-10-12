//! Integration coverage for loading committed PostgreSQL builtin schemas.

use std::path::PathBuf;

use ::clap::Parser;
use ::sql_fun_sqlast::{
    PostgresDialect, SqlFunArgs,
    sem::{BaseContext, BaseParseContext, FrozenBaseContext, FullName, TableName},
};

#[test]
fn loads_builtin_and_user_schemas() -> Result<(), Box<dyn std::error::Error>> {
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
        let builtins = PostgresDialect::parse_schema_file(
            &schema,
            &FrozenBaseContext::from(context),
            &mut None,
        )?;
        let pg_class = TableName::from(FullName::with_schema("pg_catalog", "pg_class"));
        assert!(
            builtins.get_table(&pg_class).is_some(),
            "PostgreSQL {version}"
        );
        let user_schema = tempfile::NamedTempFile::new()?;
        std::fs::write(
            user_schema.path(),
            "CREATE TABLE hoge (id int PRIMARY KEY); SELECT abs(id) + 1 AS next_id FROM hoge;",
        )?;
        let parsed = PostgresDialect::parse_schema_file(
            user_schema.path(),
            builtins.base_context(),
            &mut None,
        )?;
        let user_table = TableName::from(FullName::with_schema("public", "hoge"));
        assert!(
            parsed.get_table(&user_table).is_some(),
            "PostgreSQL {version}"
        );
    }
    Ok(())
}
