//! Errors shared by syntax conversion and semantic analysis.
use crate::ast::Dialect;
use std::{
    convert::Infallible,
    io::ErrorKind,
    path::{Path, PathBuf},
};
/// error
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    /// unknown error
    #[error("{0}")]
    GenericError(#[source] Box<dyn std::error::Error>),

    /// serde
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    /// IO Error
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Unexpected None
    #[error("unexpected None with {0}")]
    UnexpectedNone(String),

    /// Unexpected input value
    #[error("unexpected input :{0}")]
    UnexpectedInput(String),

    /// Type name is invalid
    #[error("invalid type name ")]
    TypeNameFormat(String),

    /// extension schema not found
    #[error("schema for extension {0} not found")]
    ExtensionSchemaNotFound(String),

    /// argument related errors
    #[error(transparent)]
    Arg(#[from] sql_fun_core::SqlFunArgsError),

    /// metadata related errors
    #[error(transparent)]
    Meta(#[from] sql_fun_core::MetadataError),

    /// file not found
    #[error("file not found : {0}")]
    FileNotFound(PathBuf),

    /// formatting error
    #[error(transparent)]
    FormattingError(#[from] std::fmt::Error),
}
impl AnalysisError {
    /// raise `UnexpectedNone` error
    pub fn raise_unexpected_none<T>(message: &str) -> Result<T, Self> {
        Err(Self::UnexpectedNone(String::from(message)))
    }

    /// Construct a recoverable unexpected-input error without a test-only panic.
    pub(crate) fn unexpected_input_error(message: &str) -> Self {
        Self::UnexpectedInput(message.to_owned())
    }

    /// raise `UnexpectedInput` error
    #[track_caller]
    pub fn raise_unexpected_input<T>(message: &str) -> Result<T, Self> {
        #[cfg(test)]
        panic!("unexpected input {message}");

        #[cfg_attr(test, expect(unreachable_code))]
        Err(Self::UnexpectedInput(String::from(message)))
    }

    /// type name is invalid format
    pub fn type_name_format<E: std::fmt::Debug>(value: &E) -> Self {
        Self::TypeNameFormat(format!("{value:?}"))
    }

    /// wraps `std::error::Error`
    #[track_caller]
    pub fn generic_error<E: std::error::Error + 'static + Send>(error: E) -> Self {
        #[cfg(test)]
        panic!("generic error {error}");

        #[cfg_attr(test, expect(unreachable_code))]
        Self::GenericError(Box::new(error))
    }

    /// file not found
    pub fn file_not_found<P>(file_path: P) -> Result<Infallible, Self>
    where
        P: AsRef<Path>,
    {
        Err(Self::FileNotFound(file_path.as_ref().to_path_buf()))
    }

    /// never returns from this
    pub fn never() -> ! {
        unreachable!("this function calls with Ok(Infallble) instantiated")
    }

    /// unexpected eof error
    pub fn unexpected_eof() -> Self {
        Self::Io(std::io::Error::new(
            ErrorKind::UnexpectedEof,
            "unexpected eof",
        ))
    }
}

impl AnalysisError {
    fn dialect_mismatch(parsed: Dialect, configured: Dialect) -> Self {
        Self::UnexpectedInput(format!(
            "statement dialect {parsed} does not match analysis metadata dialect {configured}"
        ))
    }
    pub(crate) fn check_dialect(parsed: Dialect, configured: Dialect) -> Result<(), Self> {
        if parsed != configured {
            return Err(Self::dialect_mismatch(parsed, configured));
        }
        Ok(())
    }
}
