pub use crate::error::AnalysisError;
use crate::{ast::Dialect, sem::CreateCompositeType};
use std::convert::Infallible;

use crate::sem::{AnalysisProblem, ParseContext, TypeReference};

impl AnalysisError {
    /// registering dynamic type to context
    pub fn raise_register_dynamic_type(
        _create_comp_type: &CreateCompositeType,
    ) -> Result<Infallible, Self> {
        Self::raise_unexpected_input("anonymous composite cannot be registered in the catalog")
    }
    /// inconsistent type reference
    #[track_caller]
    #[must_use]
    pub fn type_canonicalize_failed(_expr_type: &TypeReference) -> Self {
        todo!()
    }
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

/// problem like error, internal error for Analysis
#[derive(Debug, thiserror::Error)]
pub enum ProblemLikeError {
    /// invalid operator name
    #[error("invalid operator name {0:?}")]
    InvalidOperatorName(Vec<String>),
    /// Parser-accepted database qualification on an operator.
    #[error("cross-database operator reference {0:?}")]
    CrossDatabaseOperatorReference(Vec<String>),
}

impl ProblemLikeError {
    pub const fn invalid_operator_name(value: Vec<String>) -> Self {
        Self::InvalidOperatorName(value)
    }

    pub(crate) fn cross_database_operator_reference(parts: &[String]) -> Self {
        Self::CrossDatabaseOperatorReference(parts.to_vec())
    }

    pub fn into_problem(self) -> AnalysisProblem {
        match self {
            Self::InvalidOperatorName(v) => AnalysisProblem::operator_name_syntax(&v),
            Self::CrossDatabaseOperatorReference(v) => {
                AnalysisProblem::cross_database_operator_reference(&v)
            }
        }
    }

    pub fn report_problem_to<TContext>(self, context: &mut TContext) -> Result<(), AnalysisError>
    where
        TContext: ParseContext,
    {
        context.report_analysis_problem(self.into_problem())?;
        Ok(())
    }
}
