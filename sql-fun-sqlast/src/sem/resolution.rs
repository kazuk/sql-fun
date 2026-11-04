//! Internal short-circuiting of one invalid construct, distinct from engine failure.
use crate::{
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem,
        problem::{RuleProblem, TriggerProblem, ViewProblem},
    },
};

pub(crate) enum ResolutionFailure {
    Problem(Box<AnalysisProblem>),
    Reported,
    Fatal(AnalysisError),
}
impl ResolutionFailure {
    pub(crate) fn rule(problem: &RuleProblem) -> Self {
        Self::Problem(Box::new(AnalysisProblem::rule(problem)))
    }
    pub(crate) fn trigger(problem: &TriggerProblem) -> Self {
        Self::Problem(Box::new(AnalysisProblem::trigger(problem)))
    }
    pub(crate) fn view(problem: &ViewProblem) -> Self {
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

#[cfg(test)]
mod test_finish {
    use crate::{
        sem::{AnalysisError, ResolutionFailure},
        test_helpers::TestParseContext,
    };

    #[test]
    fn internal_contract_failure_is_propagated_without_a_semantic_problem() {
        let mut context = TestParseContext::default();
        let result: Result<(), ResolutionFailure> = Err(AnalysisError::unexpected_input_error(
            "projection analysis contract failed",
        )
        .into());
        let error = ResolutionFailure::finish(result, &mut context).unwrap_err();
        assert!(
            matches!(error, AnalysisError::UnexpectedInput(ref message) if message == "projection analysis contract failed")
        );
        assert_eq!(context.reported_problem_count(), 0);
    }
}
impl From<AnalysisError> for ResolutionFailure {
    fn from(error: AnalysisError) -> Self {
        Self::Fatal(error)
    }
}
