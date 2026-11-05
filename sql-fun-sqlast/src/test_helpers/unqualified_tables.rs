use crate::{
    ast::CreateTableMode,
    context::{ParseContext as _, SchemaFileContext},
    names::{FullName, SchemaName, TableName},
    sem::{AnalysisError, CreateTable},
    test_helpers::{TestParseContext, schema_layer, source_span},
};

pub(crate) fn unqualified_tables() -> Result<SchemaFileContext, AnalysisError> {
    let root = TestParseContext::default().freeze();
    let mut context = schema_layer(std::path::Path::new("parent.sql"), &root);
    let table = CreateTable::new(
        &TableName::from("items".parse::<FullName>()?),
        Vec::new(),
        CreateTableMode::from_if_not_exists(false),
        false,
        &source_span(),
    );
    for schema in ["a", "b"] {
        context.set_search_path(&[SchemaName::from(schema)]);
        context = context.apply_create_table(&table)?;
    }
    Ok(context)
}

pub(crate) fn set_search_path(context: &mut SchemaFileContext, path: &[&str]) {
    let mut schemas = Vec::new();
    for schema in path {
        schemas.push(SchemaName::from(schema));
    }
    context.set_search_path(&schemas);
}
