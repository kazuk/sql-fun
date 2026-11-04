//! Lightweight explicit catalog for CHECK schema transition contracts.
use crate::{
    ast::Dialect,
    context::{ParseContext, SchemaFileContext},
    names::FullName,
    sem::{AnalysisError, AstAndContextPair, TypeReference},
    test_helpers::{
        scalar_catalog::{categorized_scalar_catalog, schema_from_base},
        statements,
    },
};
pub(crate) fn context(dialect: Dialect) -> SchemaFileContext {
    let mut base = categorized_scalar_catalog(dialect);
    if dialect == Dialect::sqlite() {
        base.setup_type(TypeReference::from_full_name(FullName::with_schema(
            "public", "integer",
        )));
    }
    base.setup_binary("<", "int4", "int4", "bool", true);
    base.setup_binary("=", "int4", "int4", "bool", true);
    schema_from_base(base, std::path::Path::new("checks.sql"))
}
pub(crate) fn apply(
    context: SchemaFileContext,
    sql: &str,
) -> Result<AstAndContextPair<SchemaFileContext>, AnalysisError> {
    let dialect = context.dialect();
    statements::apply(context, sql, dialect)
}
