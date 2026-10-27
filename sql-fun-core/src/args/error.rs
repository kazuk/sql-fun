use std::path::{Path, PathBuf};

use crate::{ExtensionConfigError, MetadataError, args::ConfigurationKind};

/// Error definition for [`crate::SqlFunArgs`]
#[derive(Debug, thiserror::Error)]
pub enum SqlFunArgsError {
    /// An explicit sql-fun home path was empty.
    #[error("sql-fun home path must not be empty")]
    EmptyHomePath,

    /// The working directory needed for a relative home path was unavailable.
    #[error("cannot resolve relative sql-fun home: current working directory unavailable: {0}")]
    CurrentDirectoryUnavailable(#[source] std::io::Error),
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

    /// The user home directory is unavailable or is not absolute.
    #[error(
        "user home directory unavailable; set HOME to an absolute path or supply an absolute sql-fun home path"
    )]
    UserHomeNotDefined(),

    /// Environment variable access error
    #[error("Environment variable access error : {0}")]
    EnvVar(#[from] std::env::VarError),

    /// Metadata error
    #[error(transparent)]
    Metadata(#[from] MetadataError),

    /// Invalid `PostgreSQL` engine version specified
    #[error("Invalid PostgreSQL engine version specified as {0}")]
    InvalidPostgresEngineVersion(String),

    /// `InsufficientConfiguration` in metadata / args
    #[error("Insufficient configuration in metadata and arguments : {0} not specified")]
    InsufficientConfiguration(ConfigurationKind),

    /// Parse error in `PostgreSQL` extensions
    #[error("parse error {0} : {1}")]
    PgExtensionsParse(String, String),

    /// `PostgreSQL` extension related errors
    #[error(transparent)]
    ExtensionConfig(#[from] ExtensionConfigError),

    /// Home for `sql-fun` not exists
    #[error("home of sql-fun not eists : {0}")]
    SqlFunHomeNotExist(PathBuf),

    /// Argument or environment variable parse error
    #[error(transparent)]
    Clap(#[from] clap::Error),

    /// specified datasource not found
    #[error("Specified datasource not found {0}")]
    DatasourceNotFound(String),
}

impl From<std::io::Error> for SqlFunArgsError {
    #[track_caller]
    fn from(value: std::io::Error) -> Self {
        log::error!("IO Error: {value}");
        Self::Io(value)
    }
}

impl SqlFunArgsError {
    /// Reject an empty explicit home path.
    ///
    /// # Errors
    /// Always returns [`Self::EmptyHomePath`].
    #[track_caller]
    pub const fn empty_home_path<T>() -> Result<T, Self> {
        Err(Self::EmptyHomePath)
    }

    /// Report that the working directory required by a relative path is unavailable.
    #[track_caller]
    #[must_use]
    pub const fn current_directory_unavailable(error: std::io::Error) -> Self {
        Self::CurrentDirectoryUnavailable(error)
    }

    /// `std::env::home_dir` does not provide an absolute path.
    ///
    /// # Errors
    ///
    /// Always returns [`SqlFunArgsError::UserHomeNotDefined`].
    #[track_caller]
    pub fn user_home_not_defined<T>() -> Result<T, Self> {
        log::error!("std::env::home_dir did not provide an absolute path");
        Err(Self::UserHomeNotDefined())
    }

    /// `sql-fun` home not exists
    ///
    /// # Errors
    ///
    /// Always returns [`SqlFunArgsError::SqlFunHomeNotExist`].
    #[track_caller]
    pub fn home_not_exist<T, P: AsRef<Path>>(path: P) -> Result<T, Self> {
        Err(Self::SqlFunHomeNotExist(path.as_ref().to_path_buf()))
    }

    /// raize `FilePathNotFound`
    ///
    /// # Errors
    ///
    /// Always returns [`SqlFunArgsError::FilePathNotFound`] outside of tests.
    ///
    /// # Panics
    ///
    /// Panics in tests to surface a missing fixture path immediately.
    #[track_caller]
    pub fn file_path_not_found<T, P: AsRef<Path> + std::fmt::Debug>(path: P) -> Result<T, Self> {
        #[cfg(test)]
        panic!("Not found {path:?}");

        #[cfg(not(test))]
        Err(Self::FilePathNotFound(path.as_ref().to_path_buf()))
    }

    /// not supported value used
    ///
    /// # Errors
    ///
    /// Always returns [`SqlFunArgsError::NotSupported`].
    pub fn not_supported<T>(value: &str, category: &str) -> Result<T, Self> {
        log::error!("Not supported value {value} for {category}.");
        Err(Self::NotSupported(category.to_string(), value.to_string()))
    }

    /// invalid postgres engine version specified
    ///
    /// # Errors
    ///
    /// Always returns [`SqlFunArgsError::InvalidPostgresEngineVersion`].
    pub fn invalid_engine_version<T>(value: &str) -> Result<T, Self> {
        log::error!("Invalid PostgreSQL engine version {value}");
        Err(Self::InvalidPostgresEngineVersion(value.to_string()))
    }

    /// postgres engine version not specified
    ///
    /// # Errors
    ///
    /// Always returns [`SqlFunArgsError::InsufficientConfiguration`].
    pub const fn no_postgres_engine_version_specified<T>() -> Result<T, Self> {
        Err(Self::InsufficientConfiguration(
            ConfigurationKind::EngineVersion,
        ))
    }

    /// extension list parse error
    ///
    /// # Errors
    ///
    /// Always returns [`SqlFunArgsError::PgExtensionsParse`].
    pub fn parsing_postgres_extensions<T>(message: &str, source: &str) -> Result<T, Self> {
        Err(Self::PgExtensionsParse(
            message.to_string(),
            source.to_string(),
        ))
    }

    /// Datasource not found
    ///
    /// # Errors
    ///
    /// Always returns [`SqlFunArgsError::DatasourceNotFound`]
    ///
    pub fn datasource_not_found<T>(datasource_name: &str) -> Result<T, Self> {
        Err(Self::DatasourceNotFound(String::from(datasource_name)))
    }
}
