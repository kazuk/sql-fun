use ::sql_fun_core::SqlFunArgs;
use ::sql_fun_sqlast::{
    AstAndContextPair, PostgresDialect,
    ast::Dialect,
    parse_statements,
    sem::{BaseContext, FrozenBaseContext, SchemaFileContext},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Load real `PostgreSQL` 18 metadata per test process. The full SQL schema
/// is parsed only for the catalog integration case; no setup is shared.
pub(crate) fn analyze(
    sql: &str,
    full_catalog: bool,
) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
    let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
    let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
        "sqlfun",
        "--metadata-file",
        metadata.to_str().unwrap(),
        "--sql-fun-home",
        env!("SQL_FUN_HOME"),
        "--postgres-version",
        "18",
    ])?;
    let selected = args.load_metadata()?.remove("default").unwrap();
    let mut base = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
    if full_catalog {
        let file = PathBuf::from(env!("SQL_FUN_HOME")).join("postgres/18/schema.sql");
        let schema = PostgresDialect::parse_schema_file(&file, &base, &mut None, &args, &selected)?;
        let mut counts = BTreeMap::new();
        for problem in schema.problems() {
            *counts.entry(problem.code()).or_insert(0_usize) += 1;
            // Unrelated catalog failures remain visible. Quantified array
            // comparisons in the shipped catalog must resolve.
            let encoded = ::serde_json::to_value(problem)?;
            assert_ne!(encoded["UndefinedOperator"][0], "AexprOpAny");
            assert_ne!(encoded["UndefinedOperator"][0], "AexprOpAll");
        }
        eprintln!("PostgreSQL 18 shipped catalog diagnostics: {counts:?}");
        base = schema.base_context().clone();
    }
    let context = SchemaFileContext::new(Path::new("query.sql"), &base, &args, &selected);
    Ok(parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context)?)
}
