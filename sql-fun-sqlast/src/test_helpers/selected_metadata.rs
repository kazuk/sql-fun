use crate::context::BaseParseContext;
#[cfg(feature = "sqlite")]
use crate::context::FrozenBaseContext;
#[cfg(feature = "sqlite")]
use crate::context::SchemaFileContext;
#[cfg(feature = "sqlite")]
use ::sql_fun_core::SqlFunArgs;
use ::sql_fun_core::SqlFunMetadata;
use ::sql_fun_core::SqlFunMetadataCollection;
use std::path::Path;

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite_compatible_engine() -> Result<SqlFunMetadata, ::serde_json::Error> {
    ::serde_json::from_value(::serde_json::json!({
        "engine": "custom-sqlite-engine", "dialect": "SQLite", "version": "3"
    }))
}

#[cfg(feature = "sqlite")]
pub(crate) fn context_over_catalog(
    base: &FrozenBaseContext,
    metadata: &SqlFunMetadata,
) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    let args = SqlFunArgs::builder(&std::collections::HashMap::new())?
        .sql_dialect(::sql_fun_core::SqlDialect::PostgreSQL)
        .build()?;
    Ok(SchemaFileContext::new(
        Path::new("query.sql"),
        base,
        &args,
        metadata,
    ))
}

pub(crate) fn selected_metadata(
    fixture: &str,
) -> Result<SqlFunMetadata, Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    let mut collection = SqlFunMetadataCollection::load_from(path)?;
    Ok(collection.remove("analytics").expect("selected metadata"))
}

pub(crate) fn assert_selected_metadata<C: BaseParseContext>(
    context: &C,
    selected: &SqlFunMetadata,
) {
    let metadata = context.metadata();
    assert_eq!(metadata.name(), "analytics");
    assert_eq!(metadata.sql_dialect(), selected.sql_dialect());
    assert_eq!(metadata.engine_version(), selected.engine_version());
    assert_eq!(metadata.search_path(), selected.search_path());
    assert_eq!(metadata.connector(), selected.connector());
    assert_eq!(
        context.get_search_path()[0].as_str(),
        selected.search_path().as_ref().expect("search path")[0]
    );
}
