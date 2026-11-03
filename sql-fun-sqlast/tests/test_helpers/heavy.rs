use crate::test_helpers::create_cache_context;
use ::sql_fun_core::SqlFunArgs;
use ::sql_fun_sqlast::{
    AstAndContextPair,
    ast::Dialect,
    context::{BaseContext, FrozenBaseContext, SchemaFileContext},
    parse_statements,
};
use std::path::{Path, PathBuf};

/// Load real `PostgreSQL` 18 metadata per test process; no setup is shared.
pub(crate) fn analyze(
    sql: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
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
    let base = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
    let context = SchemaFileContext::new(Path::new("query.sql"), &base, &args, &selected);
    Ok(parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context, &mut cache_context)?)
}
