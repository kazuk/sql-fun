use ::sql_fun_sqlast::{
    AstAndContextPair, PostgresDialect, SqlFunArgs,
    ast::Dialect,
    context::{BaseContext, FrozenBaseContext, SchemaFileContext},
    parse_statements,
    sem::SemAst,
};
pub(crate) mod domain_casts;
pub(crate) mod extracted_artifacts;
pub(crate) mod parameter_resolution;
pub(crate) mod qualified_columns;
pub(crate) mod query_resolution;
pub(crate) mod schema_context_chain;
pub(crate) mod schema_file_loading;
pub(crate) mod type_metadata_regressions;
pub(crate) mod writable_views;

use crate::test_helpers::create_cache_context;

use std::path::{Path, PathBuf};

/// Loads shipped builtin JSON metadata and optional SQL definitions per call.
/// Initialization repeats for each test process; no cache is shared.
pub(crate) fn analyze(
    version: &str,
    sql: &str,
    definitions: &[&str],
) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
    let (args, selected) = environment(version)?;
    let mut base = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
    if !definitions.is_empty() {
        let file = ::tempfile::NamedTempFile::new()?;
        std::fs::write(file.path(), definitions.join(";"))?;
        let schema =
            PostgresDialect::parse_schema_file(file.path(), &base, &mut None, &args, &selected)?;
        assert!(schema.problems().is_empty(), "{:?}", schema.problems());
        base = schema.base_context().clone();
    }
    let context = SchemaFileContext::new(Path::new("query.sql"), &base, &args, &selected);
    Ok(SemAst::from_ast(
        &parse_statements(sql, Dialect::postgres()).next().unwrap()?,
        context,
        &mut cache_context,
    )?)
}

pub(crate) fn environment(
    version: &str,
) -> Result<(SqlFunArgs, ::sql_fun_core::SqlFunMetadata), Box<dyn std::error::Error>> {
    let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
    let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
        "sqlfun",
        "--metadata-file",
        metadata.to_str().unwrap(),
        "--sql-fun-home",
        env!("SQL_FUN_HOME"),
        "--postgres-version",
        version,
    ])?;
    let mut collection = args.load_metadata()?;
    Ok((
        args,
        collection
            .remove("default")
            .expect("selected test metadata"),
    ))
}
