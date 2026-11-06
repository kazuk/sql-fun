use ::sql_fun_sqlast::{
    CacheContext, SqlFunArgs,
    ast::Dialect,
    context::{BaseContext, FrozenBaseContext, SchemaFileContext},
    parse_statements,
    sem::{AnalysisError, SemAst},
};
use std::path::PathBuf;

/// Loads builtin JSON catalogs for each invocation to check the public
/// analysis boundary. Initialization is repeated; no cache is shared.
pub(crate) fn analyze(sql: &str) -> Result<(), AnalysisError> {
    let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
    let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
        "sqlfun",
        "--metadata-file",
        metadata.to_str().expect("metadata path"),
        "--sql-fun-home",
        env!("SQL_FUN_HOME"),
        "--postgres-version",
        "17",
    ])
    .expect("fixture arguments");
    let collection = args.load_metadata().expect("fixture metadata");
    let selected = collection.get("default").expect("selected test metadata");
    let base = BaseContext::new(&args, selected).expect("builtin catalog fixture");
    let context = {
        let analysis_base = &FrozenBaseContext::from(base);
        SchemaFileContext::new(&metadata, analysis_base, &args, selected)
    };
    let syntax = parse_statements(sql, Dialect::postgres())
        .next()
        .expect("SELECT fixture")
        .expect("legal SELECT syntax");
    SemAst::from_ast(&syntax, context, &mut CacheContext::new()).map(|_| ())
}
