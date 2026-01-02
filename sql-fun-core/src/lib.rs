#![deny(missing_docs)]

//! sql-fun-core library
//!
//! This library defines sql-fun's internal communication data objects.

mod args;
mod builtin;
mod coll;
mod extensions;
mod metadata;

use std::{fmt::Display, str::FromStr};

pub use self::{
    args::{CliSubCommand, DaemonControlArgs, InitializeArgs, SqlFunArgs, SqlFunArgsError},
    builtin::PostgresBuiltinPaths,
    coll::IVec,
    extensions::{
        ExtensionConfigError, ExtensionNameVersionPair, ExtensionVersion,
        PostgresExtensionsCollection,
    },
    metadata::{MetadataError, SqlFunMetadata},
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

    #[cfg(feature = "beta_postgres_versions")]
    #[serde(rename = "18")]
    Ver18,
}

impl FromStr for PostgresVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "15" => Ok(PostgresVersion::Ver15),
            "16" => Ok(PostgresVersion::Ver16),
            "17" => Ok(PostgresVersion::Ver17),
            #[cfg(feature = "beta_postgres_versions")]
            "18" => Ok(PostgresVersion::Ver18),
            _ => Err(s.to_string()),
        }
    }
}

impl Display for PostgresVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            PostgresVersion::Ver15 => "15",
            PostgresVersion::Ver16 => "16",
            PostgresVersion::Ver17 => "17",
            #[cfg(feature = "beta_postgres_versions")]
            PostgresVersion::Ver18 => "18",
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
            SqlDialect::PostgreSQL => write!(f, "postgresql"),
        }
    }
}
