use crate::payloads::CollectionReference;
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

    /// Project dump file; required only when the selected prefix includes it.
    schema_dump: Option<PathBuf>,

    /// Expected SHA-256 of the original dump text, supplied by the client.
    expected_hash: Option<String>,

    /// Requested prefix; omission preserves full-project initialization.
    #[serde(default)]
    schema_level: Option<::sql_fun_core::SchemaLevel>,

    /// Configuration transferred by `sql_fun_core::SqlFunArgs::environments`.
    environments: HashMap<String, String>,
}

impl InitializeSchemaArgs {
    /// Select schema input and transfer its effective configuration to the server.
    #[must_use]
    pub fn new(
        datasource: &str,
        schema_dump: &std::path::Path,
        expected_hash: &str,
        environments: &HashMap<String, String>,
    ) -> Self {
        Self {
            datasource: datasource.to_owned(),
            schema_dump: Some(schema_dump.to_path_buf()),
            expected_hash: Some(expected_hash.to_owned()),
            schema_level: None,
            environments: environments.clone(),
        }
    }
    /// Select a schema prefix with optional project-only inputs.
    #[must_use]
    pub fn for_level(
        datasource: &str,
        schema_dump: Option<&std::path::Path>,
        expected_hash: Option<&str>,
        environments: &HashMap<String, String>,
        level: ::sql_fun_core::SchemaLevel,
    ) -> Self {
        Self {
            datasource: datasource.to_owned(),
            schema_dump: schema_dump.map(std::path::Path::to_path_buf),
            expected_hash: expected_hash.map(str::to_owned),
            environments: environments.clone(),
            schema_level: Some(level),
        }
    }
    /// Effective prefix, preserving existing clients' full-project behavior.
    #[must_use]
    pub fn level(&self) -> ::sql_fun_core::SchemaLevel {
        self.schema_level.unwrap_or_default()
    }
}

/// schema context
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct SchemaContext {
    /// context handle
    context_handle: Option<ContextHandle>,

    /// SHA-256 of the project dump, absent for lower schema prefixes.
    schema_dump_hash: Option<String>,

    /// schema analysis problems
    problems: Collection<AnalysisProbrem>,
}

impl SchemaContext {
    /// Construct successful initialization metadata and its first diagnostic page.
    /// Takes ownership of the page to avoid copying its items.
    #[must_use]
    pub fn new(
        context_handle: &ContextHandle,
        schema_dump_hash: Option<&str>,
        problems: Collection<AnalysisProbrem>,
    ) -> Self {
        Self {
            context_handle: Some(context_handle.clone()),
            schema_dump_hash: schema_dump_hash.map(str::to_owned),
            problems,
        }
    }
}

impl super::CollectionTraversal for SchemaContext {
    fn collections(&self) -> Vec<&dyn CollectionReference> {
        vec![&self.problems]
    }
    fn collections_mut(&mut self) -> Vec<&mut dyn CollectionReference> {
        vec![&mut self.problems]
    }
}
