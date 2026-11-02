#![deny(missing_docs)]

//! Shared configuration, resource resolution, and communication types for SQL-Fun.
//!
//! # Configuration ownership
//!
//! This crate owns interpretation of [`SqlFunArgs`], environment-based settings,
//! and datasource metadata. Its APIs determine the effective configuration and
//! precedence between argument overrides and metadata, including SQL dialect,
//! engine version, extensions, and search path. It also resolves SQL-Fun home
//! resources, `PostgreSQL` builtin catalogs, and extension SQL files.
//!
//! # Configuration transfer
//!
//! Configuration crosses both RPC and child-process boundaries as an environment
//! variable [`HashMap<String, String>`](std::collections::HashMap).
//! [`SqlFunArgs::environments`] serializes the selected configuration into this
//! map. [`SqlFunArgs::from_environments`] restores it using core's environment
//! parser without consulting the receiver's ambient process environment. The
//! transferred map must explicitly select a nonempty SQL-Fun home and metadata
//! file; missing selections are errors rather than permission to discover assets.
//! The same map can be supplied to [`std::process::Command::envs`] when starting
//! a child process.
//!
//! # Consumer boundary
//!
//! Other crates, especially the server, consume these APIs rather than interpret
//! environment keys, reproduce metadata semantics, or reconstruct configuration
//! from individual settings. A receiver restores [`SqlFunArgs`], requests the
//! named datasource through [`SqlFunArgs::selected_metadata`], and uses core's
//! accessors to obtain its effective configuration and resource paths. The
//! consumer owns its transport and processing responsibilities, such as RPC
//! validation, schema loading, and session state. Keeping configuration decisions
//! in core lets arguments and metadata evolve without duplicating their handling
//! in server initialization or other consumers.

mod access_kind;
/// Structured semantic diagnostics without terminal presentation.
pub mod analysis_problem;
pub use self::access_kind::AccessKind;
mod args;
mod builtin;
mod codegen_options;
mod coll;
mod cte_catalog;
mod diagnostic_presentation;
mod extensions;
mod highlighter;
mod lookup_result;
mod metadata;
mod placehlodered_sql;
mod postgres_version;
mod rewrite_options;
mod schema_level;
mod sql_tokens;
pub use self::schema_level::SchemaLevel;

/// `sql-fun` code generator lockfile
pub mod lockfile;

use sha2::{Digest, Sha256};

pub use self::{
    cte_catalog::{
        CatalogFileIndex, CteAlias, CteCatalog, CteCatalogCheckError, CteCatalogEntry,
        CteCatalogError, CteCatalogReference, CteName, CteParameter, CteParameterBinding,
        CteParameterBindingCollection, CteParameterCollection, CteParameterRename,
        ParameterRenameCollection, ResolvedCteCatalog,
    },
    diagnostic_presentation::DiagnosticPresentation,
    placehlodered_sql::{
        FragmentName, ParameterName, PlaceholderTypeHint, PlaceholderedSql, PlaceholderedSqlError,
        PlaceholderedSqlToken,
    },
    sql_tokens::{SqlToken, TokenKind, TokenStream, TokenStreamError},
};

#[cfg(test)]
/// Shared helpers for crate unit tests.
pub mod test_helpers;

use std::{fmt::Display, path::Path, str::FromStr};

pub use self::{
    args::{
        InitializeArgs, SqlFunArgs, SqlFunArgsBuilder, SqlFunArgsBuilderError, SqlFunArgsError,
        resolve_sql_fun_home_path,
    },
    builtin::PostgresBuiltinPaths,
    codegen_options::{BuilderSetter, CodegenOptions},
    coll::IVec,
    extensions::{
        ExtensionConfigError, ExtensionNameVersionPair, ExtensionVersion,
        PostgresExtensionsCollection, PostgresExtensionsParseError,
    },
    highlighter::TerminalColor,
    lookup_result::LookupResult,
    metadata::{
        InitializationDatasources, MetadataError, PostgresEngineVersion, RoleInventory,
        SqlFunMetadata, SqlFunMetadataCollection,
    },
    postgres_version::{PostgresVersion, PostgresVersionInventory},
    rewrite_options::RewriteOptions,
};

/// SQL Dialect
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq)]
pub enum SqlDialect {
    /// postgres using a `pg_query` crate for parsing a syntax.
    #[serde(rename = "postgres")]
    PostgreSQL,

    /// `SQLite` using a `turso_parser` crate for parsing a syntax
    #[cfg(feature = "sqlite")]
    SQLite,
}

impl SqlDialect {
    /// Compare identifier values, after SQL parsing or explicit alias selection.
    /// `PostgreSQL` values preserve case; `SQLite` ignores ASCII case only.
    #[must_use]
    pub fn same_identifier(self, left: &str, right: &str) -> bool {
        match self {
            Self::PostgreSQL => left == right,
            #[cfg(feature = "sqlite")]
            Self::SQLite => left.eq_ignore_ascii_case(right),
        }
    }
}

impl FromStr for SqlDialect {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.to_lowercase() == "postgresql" {
            return Ok(Self::PostgreSQL);
        }
        Err(s.to_string())
    }
}

impl Display for SqlDialect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PostgreSQL => write!(f, "postgresql"),
            #[cfg(feature = "sqlite")]
            Self::SQLite => write!(f, "sqlite"),
        }
    }
}

/// lint diagnostics configurations
pub mod diagnostics {
    use indexmap::IndexMap;

    /// diagnostics level
    #[derive(Debug, serde::Deserialize, Clone, Copy, PartialEq)]
    pub enum DiagnosticsLevel {
        /// hide always
        Suppress,
        /// show only verbose flagged
        OnlyVerbose,
        /// show warning
        Warning,
        /// show as error, require fail
        Error,
    }

    /// diagnostics kind
    #[derive(Debug, serde::Deserialize, Clone, Copy, Eq, PartialEq, Hash)]
    pub enum DiagnosticsKind {
        /// table not found
        TableNotFound,
        /// column not found
        ColumnNotFound,
        /// type not found
        TypeNotFound,
    }

    impl DiagnosticsKind {
        const fn get_diagnostics_level_default(self) -> DiagnosticsLevel {
            #[expect(clippy::match_same_arms)]
            match self {
                Self::TableNotFound => DiagnosticsLevel::Warning,
                Self::ColumnNotFound => DiagnosticsLevel::Warning,
                Self::TypeNotFound => DiagnosticsLevel::Warning,
            }
        }
    }

    /// diagnostics configurations
    pub struct DiagnosticsConfigurations {
        defaults: Option<Box<Self>>,

        configurations: IndexMap<DiagnosticsKind, DiagnosticsLevel>,
    }

    impl DiagnosticsConfigurations {
        /// get diagnostics level for kind
        #[must_use]
        pub fn get_diagnostics_lebel(&self, kind: DiagnosticsKind) -> DiagnosticsLevel {
            if let Some(level) = self.configurations.get(&kind).copied() {
                level
            } else if let Some(defaults) = &self.defaults {
                defaults.get_diagnostics_lebel(kind)
            } else {
                kind.get_diagnostics_level_default()
            }
        }
    }
}

/// Compute Sha256 with hex encode
#[derive(Default)]
pub struct Hasher(Sha256);

impl Hasher {
    /// create new
    #[must_use]
    pub fn new() -> Self {
        Self(Sha256::new())
    }

    /// append hash text
    pub fn append_str(&mut self, str: &str) {
        self.0.update(str.as_bytes());
    }

    /// compute hash code
    #[must_use]
    pub fn finish(self) -> String {
        let digest = self.0.finalize();
        hex::encode(digest)
    }

    /// hash the file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read as UTF-8 text.
    pub fn hash_file<P: AsRef<Path>>(path: P) -> Result<String, std::io::Error> {
        let content = fs_err::read_to_string(path)?;
        let mut hasher = Self::new();
        hasher.append_str(&content);
        Ok(hasher.finish())
    }
}
