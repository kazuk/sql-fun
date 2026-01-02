use std::convert::Infallible;

use crate::sem::{AnalysisProblem, FullName, ParseContext, TableName, TypeReference, ViewName};

/// error
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    /// unknown errror
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

    /// Unexpeced input value
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
}

impl AnalysisError {
    /// raises `UnexpectedNone`
    #[deprecated = "use raise_unexpected_none"]
    #[track_caller]
    #[must_use]
    pub fn unexpected_none(message: &str) -> Self {
        #[cfg(test)]
        panic!("unexpected none {message}");

        #[cfg_attr(test, expect(unreachable_code))]
        AnalysisError::UnexpectedNone(String::from(message))
    }

    /// raise `UnexpectedNone` error
    pub fn raise_unexpected_none<T>(message: &str) -> Result<T, Self> {
        Err(AnalysisError::UnexpectedNone(String::from(message)))
    }

    /// raise `UnexpectedInput` error
    #[deprecated = "use raise_unexpected_input"]
    #[track_caller]
    #[must_use]
    pub fn unexpected_input(message: &str) -> Self {
        #[cfg(test)]
        panic!("unexpected input {message}");

        #[cfg_attr(test, expect(unreachable_code))]
        AnalysisError::UnexpectedInput(String::from(message))
    }

    /// raise `UnexpectedInput` error
    #[track_caller]
    pub fn raise_unexpected_input<T>(message: &str) -> Result<T, Self> {
        #[cfg(test)]
        panic!("unexpected input {message}");

        #[cfg_attr(test, expect(unreachable_code))]
        Err(AnalysisError::UnexpectedInput(String::from(message)))
    }

    /// type name is invalid format
    pub fn type_name_format<E: std::fmt::Debug>(value: &E) -> Self {
        Self::TypeNameFormat(format!("{value:?}"))
    }

    #[expect(missing_docs)]
    #[deprecated]
    #[track_caller]
    #[must_use]
    pub fn extension_schema_not_found(extension_name: &str) -> Self {
        Self::ExtensionSchemaNotFound(String::from(extension_name))
    }

    /// wraps `std::error::Error`
    #[track_caller]
    pub fn generic_error<E: std::error::Error + 'static + Send>(error: E) -> Self {
        #[cfg(test)]
        panic!("generic error {error}");

        #[cfg_attr(test, expect(unreachable_code))]
        Self::GenericError(Box::new(error))
    }

    /// inconsistent type reference
    #[track_caller]
    #[must_use]
    pub fn type_canonicalize_failed(_expr_type: &TypeReference) -> Self {
        todo!()
    }

    #[expect(missing_docs)]
    #[deprecated]
    #[track_caller]
    #[must_use]
    pub fn alter_table_not_found(_table_name: &TableName) -> Self {
        todo!()
    }

    #[expect(missing_docs)]
    #[deprecated]
    #[track_caller]
    #[must_use]
    pub fn alter_view_not_found(_view_name: &ViewName) -> Self {
        todo!()
    }

    #[expect(missing_docs)]
    #[deprecated]
    #[track_caller]
    #[must_use]
    pub fn alter_seq_not_found(_seq_name: &FullName) -> Self {
        todo!()
    }

    /// regitering dynamic type to context
    pub fn raise_register_dynamic_type(
        _create_comp_type: &super::CreateCompositType,
    ) -> Result<Infallible, Self> {
        todo!()
    }

    /// never returns from this
    pub fn never() -> ! {
        unreachable!("this function calls with Ok(Infallble) instanciated")
    }
}

/// problem like error, internal error for Analysis
#[derive(Debug, thiserror::Error)]
pub enum ProblemLikeError {
    /// invalid operator name
    #[error("invalid operator name {0:?}")]
    InvalidOperatorName(Vec<String>),
}

impl ProblemLikeError {
    pub fn invalid_operator_name(value: Vec<String>) -> Self {
        Self::InvalidOperatorName(value)
    }

    pub fn into_problem(self) -> AnalysisProblem {
        match self {
            Self::InvalidOperatorName(v) => AnalysisProblem::operator_name_syntax(&v),
        }
    }

    pub fn report_problem_to<TParseContext>(self, context: &mut TParseContext)
    where
        TParseContext: ParseContext,
    {
        context.report_problem(self.into_problem());
    }
}
