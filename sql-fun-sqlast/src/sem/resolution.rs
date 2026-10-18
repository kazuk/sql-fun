//! Internal short-circuiting of one invalid construct, distinct from engine failure.
use crate::sem::{AnalysisError, AnalysisProblem, ParseContext};

pub(crate) enum ResolutionFailure {
    Problem(Box<AnalysisProblem>),
    Reported,
    Fatal(AnalysisError),
}
impl ResolutionFailure {
    pub(crate) fn rule(problem: &crate::sem::problem::RuleProblem) -> Self {
        Self::Problem(Box::new(AnalysisProblem::rule(problem)))
    }
    pub(crate) fn trigger(problem: &crate::sem::problem::TriggerProblem) -> Self {
        Self::Problem(Box::new(AnalysisProblem::trigger(problem)))
    }
    pub(crate) fn view(problem: &crate::sem::problem::ViewProblem) -> Self {
        Self::Problem(Box::new(AnalysisProblem::view(problem)))
    }
    pub(crate) const fn reported() -> Self {
        Self::Reported
    }
    pub(crate) fn finish<T>(
        result: Result<T, Self>,
        context: &mut impl ParseContext,
    ) -> Result<Option<T>, AnalysisError> {
        match result {
            Ok(value) => Ok(Some(value)),
            Err(Self::Problem(problem)) => {
                context.report_analysis_problem(*problem)?;
                Ok(None)
            }
            Err(Self::Reported) => Ok(None),
            Err(Self::Fatal(error)) => Err(error),
        }
    }
}
impl From<AnalysisError> for ResolutionFailure {
    fn from(error: AnalysisError) -> Self {
        Self::Fatal(error)
    }
}
