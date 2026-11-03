use crate::context::ParseContext as _;
use crate::sem::AnalysisError;
use crate::test_helpers::schema_layer;

use crate::context::SchemaFileContext;
use crate::names::{FullName, SchemaName, TableName};
use crate::sem::{CreateIndex, IndexElementCollection};
use crate::test_helpers::TestParseContext;
use std::path::Path;

pub(crate) fn index(schema: &str, table: &str, name: &str) -> CreateIndex {
    CreateIndex::new(
        &FullName::with_schema(schema, name),
        &TableName::from(FullName::with_schema(schema, table)),
        &IndexElementCollection::default(),
    )
}

pub(crate) fn layer(
    path: &[&str],
    indexes: &[CreateIndex],
) -> Result<SchemaFileContext, AnalysisError> {
    let base = TestParseContext::default().freeze();
    let mut context = schema_layer(Path::new("indexes.sql"), &base);
    let mut schemas = Vec::new();
    for name in path {
        schemas.push(SchemaName::from(name));
    }
    context.set_search_path(&schemas);
    for index in indexes {
        context = context.apply_create_index(index)?;
    }
    Ok(context)
}
