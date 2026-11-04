//! Lightweight explicit catalog for CHECK schema transition contracts.
use crate::{
    ast::Dialect,
    context::{ParseContext, SchemaFileContext},
    names::FullName,
    sem::{AnalysisError, AstAndContextPair, TypeReference},
    syn::ColumnDef,
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

pub(crate) fn postgres_column(sql: &str) -> ColumnDef {
    use crate::{
        syn::{ListOpt, Opt},
        test_helpers::SynBuilder,
    };
    let statement = SynBuilder::new().parse(sql);
    let table = statement.as_create_stmt().as_inner().expect("CREATE TABLE");
    table.get_table_elts().as_inner().expect("columns")[0]
        .as_column_def()
        .as_inner()
        .expect("column declaration")
}
