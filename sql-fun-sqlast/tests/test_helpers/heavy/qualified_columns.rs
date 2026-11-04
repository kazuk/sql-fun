use crate::test_helpers::create_cache_context;
use ::sql_fun_sqlast::sem::SemAst;

use ::sql_fun_sqlast::{
    AstAndContextPair, PostgresDialect, SqlFunArgs,
    ast::Dialect,
    context::{BaseContext, FrozenBaseContext, SchemaFileContext},
    parse_statements,
};
use std::path::Path;

/// Uses the shipped builtin initialization path and an in-memory-size schema
/// per invocation to exercise the feature-enabled public SQLite analyzer.
#[cfg(feature = "sqlite")]
pub(crate) fn analyze_sqlite(
    sql: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
    let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
        "sqlfun",
        "--sql-fun-home",
        env!("SQL_FUN_HOME"),
        "--postgres-version",
        "17",
    ])?;
    let metadata = ::serde_json::from_value(
        ::serde_json::json!({"engine": "SQLite", "dialect": "SQLite", "version": "3", "search_path": ["main"]}),
    )?;
    let base = FrozenBaseContext::from(BaseContext::new(&args, &metadata)?);
    let file = ::tempfile::NamedTempFile::new()?;
    std::fs::write(file.path(), "CREATE TABLE t(id NOT NULL)")?;
    let schema = ::sql_fun_sqlast::SqliteDialect::parse_schema_file(
        file.path(),
        &base,
        &mut None,
        &args,
        &metadata,
    )?;
    assert!(schema.problems().is_empty(), "{:?}", schema.problems());
    let context = SchemaFileContext::new(
        Path::new("query.sql"),
        schema.base_context(),
        &args,
        &metadata,
    );
    Ok(SemAst::from_ast(
        &parse_statements(sql, Dialect::sqlite()).next().unwrap()?,
        context,
        &mut cache_context,
    )?)
}

/// Loads the shipped `PostgreSQL` builtin catalog per invocation. The public
/// schema/query boundary requires this catalog; initialization is not shared.
pub(crate) fn analyze(
    sql: &str,
    database: Option<&str>,
) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
    let metadata = ::tempfile::NamedTempFile::new()?;
    let database = database
        .map(|name| format!("database-name = {name:?}\n"))
        .unwrap_or_default();
    std::fs::write(
        metadata.path(),
        format!(
            "[package.metadata.database.default]\nengine = 'postgres'\ndialect = 'postgres'\nconnector = 'sqlx'\nsearch_path = ['a', 'public']\n{database}"
        ),
    )?;
    let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
        "sqlfun",
        "--metadata-file",
        metadata.path().to_str().unwrap(),
        "--sql-fun-home",
        env!("SQL_FUN_HOME"),
        "--postgres-version",
        "17",
    ])?;
    let selected = args.load_metadata()?.remove("default").unwrap();
    let base = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
    let file = ::tempfile::NamedTempFile::new()?;
    std::fs::write(
        file.path(),
        r#"CREATE SCHEMA a; CREATE SCHEMA b; CREATE SCHEMA "a.b";
CREATE TABLE a.t(id integer NOT NULL); CREATE TABLE b.t(id bigint);
CREATE TABLE a.u(id integer); CREATE TABLE "a.b"."t.x"(id integer);
CREATE VIEW a.v AS SELECT id FROM a.t;"#,
    )?;
    let schema =
        PostgresDialect::parse_schema_file(file.path(), &base, &mut None, &args, &selected)?;
    assert!(schema.problems().is_empty(), "{:?}", schema.problems());
    let context = SchemaFileContext::new(
        Path::new("query.sql"),
        schema.base_context(),
        &args,
        &selected,
    );
    Ok(SemAst::from_ast(
        &parse_statements(sql, Dialect::postgres()).next().unwrap()?,
        context,
        &mut cache_context,
    )?)
}
