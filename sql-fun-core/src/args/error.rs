use std::path::{Path, PathBuf};

use crate::{ExtensionConfigError, MetadataError, args::ConfigurationKind};

/// Error definition for [crate::SqlFunArgs]
#[derive(Debug, thiserror::Error)]
pub enum SqlFunArgsError {
    /// Not supported value
    #[error("{1} not supported for {0}")]
    NotSupported(String, String),

    /// IO Error
    #[error("Io Error {0}")]
    Io(std::io::Error),

    /// path not found
    #[error("File not found {0}")]
    FilePathNotFound(std::path::PathBuf),

    /// unexpected error
    #[error("Unexpected error: {0}")]
    Unexpected(String),

    /// HOME environment variable not defined
    #[error("HOME environment variable not defined")]
    UserHomeNotDefined(),

    /// Environment variable access error
    #[error("Environment variable access error : {0}")]
    EnvVar(#[from] std::env::VarError),

    /// Metadata error
    #[error(transparent)]
    Metadata(#[from] MetadataError),

    /// Invalid PostgreSQL enging version specified
    #[error("Invalid PostgreSQL engine version specified as {0}")]
    InvalidPostgresEngineVersion(String),

    /// InsufficientConfiguration in metadata / args
    #[error("Insufficient configuration in metadata and arguments : {0} not specified")]
    InsufficientConfiguration(ConfigurationKind),

    /// Parse error in PostgreSQL extensions
    #[error("parse error {0} : {1}")]
    PgExtenstionsParse(String, String),

    /// PostgreSQL extension related errors
    #[error(transparent)]
    ExtensionConfig(#[from] ExtensionConfigError),

    /// Home for `sql-fun` not exists
    #[error("home of sql-fun not eists : {0}")]
    SqlFunHomeNotExist(PathBuf),
}

impl From<std::io::Error> for SqlFunArgsError {
    #[track_caller]
    fn from(value: std::io::Error) -> Self {
        log::error!("IO Error: {value}");
        Self::Io(value)
    }
}

impl SqlFunArgsError {
    /// std::env::home_dir returns None
    #[track_caller]
    pub fn user_home_not_defined<T>() -> Result<T, Self> {
        log::error!("std::env::home_dir returns none");
        Err(Self::UserHomeNotDefined())
    }

    /// `sql-fun` home not exists
    #[track_caller]
    pub fn home_not_exist<T, P: AsRef<Path>>(path: P) -> Result<T, Self> {
        Err(Self::SqlFunHomeNotExist(path.as_ref().to_path_buf()))
    }

    /// raize FilePathNotFound
    #[track_caller]
    pub fn file_path_not_found<T, P: AsRef<Path> + std::fmt::Debug>(path: P) -> Result<T, Self> {
        #[cfg(test)]
        panic!("Not found {path:?}");

        #[cfg(not(test))]
        Err(Self::FilePathNotFound(path.as_ref().to_path_buf()))
    }

    /// not supported value used
    pub fn not_suppoted<T>(value: &str, category: &str) -> Result<T, Self> {
        log::error!("Not supported value {value} for {category}.");
        Err(Self::NotSupported(category.to_string(), value.to_string()))
    }

    /// invalid postgres engine version specified
    pub fn invalid_engine_version<T>(value: &str) -> Result<T, Self> {
        log::error!("Invalid PostgreSQL engine version {value}");
        Err(Self::InvalidPostgresEngineVersion(value.to_string()))
    }

    /// postgres engine version not specified
    pub fn no_postgres_engine_version_specified<T>() -> Result<T, Self> {
        Err(Self::InsufficientConfiguration(
            ConfigurationKind::EngineVersion,
        ))
    }

    /// extension list parse error
    pub fn parsing_postgres_extensions<T>(message: &str, source: &str) -> Result<T, Self> {
        Err(Self::PgExtenstionsParse(
            message.to_string(),
            source.to_string(),
        ))
    }
}
