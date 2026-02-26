use sql_fun_core::{PostgresExtensionsParseError, SqlFunArgsBuilderError, SqlFunArgsError};
use sql_fun_server_api::SqlFunServerApiError;
use sql_fun_sqlast::{ParseDumpFileError, sem::AnalysisError};

use crate::core_wrapping::InvalidEngneVersion;

/// Server API internal errors
#[derive(thiserror::Error, Debug)]
pub enum SqlFunServerError {
    #[error("invalid engine version {0}")]
    InvalidEngineVersion(InvalidEngneVersion),

    #[error("extensions parse error {0}")]
    ExtensionListParse(PostgresExtensionsParseError),

    #[error("context args can not to build {0}")]
    ArgsBuild(#[from] SqlFunArgsBuilderError),

    #[error("context argument error {0}")]
    InvalidArgument(#[from] SqlFunArgsError),

    #[error("semantic analysis failed {0}")]
    Analisys(#[from] AnalysisError),

    #[error("parse dump file failed {0}")]
    ParseDump(#[from] ParseDumpFileError),
}

impl From<InvalidEngneVersion> for SqlFunServerError {
    fn from(value: InvalidEngneVersion) -> Self {
        Self::InvalidEngineVersion(value)
    }
}

impl From<PostgresExtensionsParseError> for SqlFunServerError {
    fn from(value: PostgresExtensionsParseError) -> Self {
        Self::ExtensionListParse(value)
    }
}

impl SqlFunServerError {
    fn is_unexpected(&self) -> bool {
        matches!(self, Self::Analisys(_) | Self::ArgsBuild(_))
    }

    fn is_context_initialize(&self) -> bool {
        matches!(
            self,
            Self::InvalidArgument(_) | Self::InvalidEngineVersion(_) | Self::ExtensionListParse(_)
        )
    }
}

impl From<SqlFunServerError> for SqlFunServerApiError {
    fn from(value: SqlFunServerError) -> Self {
        if value.is_unexpected() {
            SqlFunServerApiError::new_unexpected(&value.to_string())
        } else if value.is_context_initialize() {
            SqlFunServerApiError::new_schema_context_initialize_failed(value)
        } else {
            #[cfg(test)]
            panic!("unknown error {value} no mapping defined");

            #[cfg(not(test))]
            SqlFunServerApiError::new_unexpected("unknown error")
        }
    }
}
