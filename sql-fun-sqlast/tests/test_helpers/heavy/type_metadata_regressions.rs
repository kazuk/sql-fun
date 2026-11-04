use crate::test_helpers::{create_cache_context, heavy};
use ::sql_fun_sqlast::sem::SemAst;

use ::sql_fun_sqlast::{
    AstAndContextPair, PostgresDialect,
    ast::Dialect,
    context::{BaseContext, FrozenBaseContext, SchemaFileContext},
    parse_statements,
};
use std::path::Path;

/// Loads the full builtin catalog and schema to verify context forwarding.
pub(crate) fn catalog(sql: &str) -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
    let (args, selected) = heavy::environment("17")?;
    let builtin = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
    let file = ::tempfile::NamedTempFile::new()?;
    std::fs::write(file.path(), sql)?;
    let schema =
        PostgresDialect::parse_schema_file(file.path(), &builtin, &mut None, &args, &selected)?;
    assert!(schema.problems().is_empty(), "{:?}", schema.problems());
    Ok(schema.base_context().clone())
}

pub(crate) fn analyze(
    schema: &str,
    sql: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
    let base = catalog(schema)?;
    let (args, selected) = heavy::environment("17")?;
    let context = SchemaFileContext::new(Path::new("query.sql"), &base, &args, &selected);
    Ok(SemAst::from_ast(
        &parse_statements(sql, Dialect::postgres()).next().unwrap()?,
        context,
        &mut cache_context,
    )?)
}
