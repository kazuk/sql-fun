use crate::ast::Dialect;
use crate::test_helpers::scalar_catalog::{scalar_catalog, schema_from_base};

use crate::context::SchemaFileContext;

/// Integer/boolean/text identity conversions for COALESCE contracts.
/// No builtin catalog parsing is needed for these condition/structure contracts.
pub(crate) fn context() -> SchemaFileContext {
    context_for(Dialect::postgres())
}

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_context() -> SchemaFileContext {
    context_for(Dialect::sqlite())
}

fn context_for(dialect: Dialect) -> SchemaFileContext {
    let base = scalar_catalog(dialect);

    schema_from_base(base, std::path::Path::new("query.sql"))
}
