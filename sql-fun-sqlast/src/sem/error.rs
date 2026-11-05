pub use crate::error::AnalysisError;

use crate::sem::{AnalysisProblem, ParseContext};

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
