use std::path::PathBuf;

use crate::{AnalysisProbrem, Collection, ContextHandle};

/// Engine dialect selecting SQL syntax perser.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum EngineDialect {
    /// `PostgreSQL` valiant, selects parser as `pg_query`
    PostgreSQL,
}

/// Selecting database builtin objects
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum EngineVersion {
    /// `PostgreSQL` and version string.
    PostgreSQL(String),
}

/// `PostgreSQL` extension
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ExtensionSpec {
    /// Name for extension
    extension_name: String,
    /// Version for extension
    extension_version: String,
}

/// Load schema dump and initialize semantic analysis context
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct InitializeSchemaArgs {
    /// SQL dialect for schema
    engine_dialect: EngineDialect,

    /// Used for loading builtin schema objects
    engine_version: EngineVersion,

    /// Enabled extensions
    extensions: Vec<ExtensionSpec>,

    /// Search path
    search_path: Vec<String>,

    /// schema dump file
    schema_dump: PathBuf,
}

/// schema context
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct SchemaContext {
    /// context handle
    context_handle: Option<ContextHandle>,

    /// SHA 256 hash for schema dump
    schema_dump_hash: String,

    /// schema analysis problems
    problems: Collection<AnalysisProbrem>,
}
