use std::{collections::HashMap, path::PathBuf};

use crate::{
    Collection,
    payloads::{AnalysisProbrem, ContextHandle},
};

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
    /// datasource name
    datasource: String,

    /// schema dump file
    schema_dump: PathBuf,

    /// Expected SHA-256 of the original dump text, supplied by the client.
    expected_hash: String,

    /// Configuration transferred by `sql_fun_core::SqlFunArgs::environments`.
    environments: HashMap<String, String>,
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

impl SchemaContext {
    /// Construct successful initialization metadata and its first diagnostic page.
    /// Takes ownership of the page to avoid copying its items.
    #[must_use]
    pub fn new(
        context_handle: &ContextHandle,
        schema_dump_hash: &str,
        problems: Collection<AnalysisProbrem>,
    ) -> Self {
        Self {
            context_handle: Some(context_handle.clone()),
            schema_dump_hash: schema_dump_hash.to_owned(),
            problems,
        }
    }
}

impl super::CollectionTraversal for SchemaContext {
    fn collections(&self) -> Vec<&dyn super::CollectionReference> {
        vec![&self.problems]
    }
    fn collections_mut(&mut self) -> Vec<&mut dyn super::CollectionReference> {
        vec![&mut self.problems]
    }
}
