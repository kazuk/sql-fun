#![deny(missing_docs)]

//! sql-fun-core library
//!
//! This library defines sql-fun's internal communication data objects.

mod args;
mod builtin;
mod codegen_options;
mod coll;
mod cte_catalog;
mod extensions;
mod highlighter;
mod metadata;
mod placehlodered_sql;
mod postgres_version;

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
    placehlodered_sql::{
        FragmentName, ParameterName, PlaceholderTypeHint, PlaceholderedSql, PlaceholderedSqlError,
        PlaceholderedSqlToken,
    },
};

#[cfg(test)]
/// Shared helpers for crate unit tests.
pub mod test_helpers;

use std::{fmt::Display, path::Path, str::FromStr};

pub use self::{
    args::{
        InitializeArgs, SqlFunArgs, SqlFunArgsBuilder, SqlFunArgsBuilderError, SqlFunArgsError,
    },
    builtin::PostgresBuiltinPaths,
    codegen_options::{BuilderSetter, CodegenOptions},
    coll::IVec,
    extensions::{
        ExtensionConfigError, ExtensionNameVersionPair, ExtensionVersion,
        PostgresExtensionsCollection, PostgresExtensionsParseError,
    },
    highlighter::{HighlighterStyle, HighlighterTheme, HighlighterThemeError, TerminalColor},
    metadata::{
        InitializationDatasources, MetadataError, PostgresEngineVersion, SqlFunMetadata,
        SqlFunMetadataCollection,
    },
    postgres_version::{PostgresVersion, PostgresVersionInventory},
};

/// SQL Dialect
#[derive(Debug, serde::Deserialize, Clone, Copy, PartialEq)]
pub enum SqlDialect {
    /// postgres using a `pg_query` crate for parsing a syntax.
    #[serde(rename = "postgres")]
    PostgreSQL,

    /// `SQLite` using a `turso_parser` crate for parsing a syntax
    #[cfg(feature = "sqlite")]
    SQLite,
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
