#![deny(missing_docs)]

//! sql-fun-core library
//!
//! This library defines sql-fun's internal communication data objects.

mod args;
mod builtin;
mod coll;
mod extensions;
mod highlighter;
mod metadata;
#[cfg(test)]
/// Shared helpers for crate unit tests.
pub mod test_helpers;

use std::{fmt::Display, str::FromStr};

pub use self::{
    args::{
        InitializeArgs, SqlFunArgs, SqlFunArgsBuilder, SqlFunArgsBuilderError, SqlFunArgsError,
    },
    builtin::PostgresBuiltinPaths,
    coll::IVec,
    extensions::{
        ExtensionConfigError, ExtensionNameVersionPair, ExtensionVersion,
        PostgresExtensionsCollection, PostgresExtensionsParseError,
    },
    highlighter::{HighlighterStyle, HighlighterTheme, HighlighterThemeError, TerminalColor},
    metadata::{MetadataError, PostgresEngineVersion, SqlFunMetadata},
};

/// Supporting `PostgreSQL` Database engine versions
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq)]
pub enum PostgresVersion {
    /// `PostgreSQL` version 15
    #[serde(rename = "15")]
    Ver15,
    /// `PostgreSQL` version 16
    #[serde(rename = "16")]
    Ver16,
    /// `PostgreSQL` version 17
    #[serde(rename = "17")]
    Ver17,

    /// `PostgreSQL` version 18
    #[serde(rename = "18")]
    Ver18,
}

impl FromStr for PostgresVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "15" => Ok(Self::Ver15),
            "16" => Ok(Self::Ver16),
            "17" => Ok(Self::Ver17),
            #[cfg(feature = "beta_postgres_versions")]
            "18" => Ok(PostgresVersion::Ver18),
            _ => Err(s.to_string()),
        }
    }
}

impl Display for PostgresVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Ver15 => "15",
            Self::Ver16 => "16",
            Self::Ver17 => "17",
            Self::Ver18 => "18",
        };
        f.write_str(value)
    }
}

/// SQL Dialect
#[derive(Debug, serde::Deserialize, Clone, Copy, PartialEq)]
pub enum SqlDialect {
    /// postgres using a `pg_query` crate for parsing a syntax.
    #[serde(rename = "postgres")]
    PostgreSQL,
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
        }
    }
}
