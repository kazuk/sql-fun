//! Per-analysis bookkeeping owned by the common parse context.

use crate::sem::AnalysisProblem;

/// Temporary bookkeeping for one outer statement or standalone expression.
///
/// This contains no configuration or catalog. Nested resolvers share the owning
/// `ParseContext`; only its outer analysis boundary starts and completes a run.
#[derive(Debug, Default)]
pub struct AnalysisState {
    active: bool,
    has_parameters: bool,
    problems: Vec<AnalysisProblem>,
}

impl AnalysisState {
    pub(crate) const fn has_problems(&self) -> bool {
        !self.problems.is_empty()
    }
    pub(crate) fn begin(&mut self) {
        *self = Self {
            active: true,
            ..Self::default()
        };
    }

    pub(crate) const fn active(&self) -> bool {
        self.active
    }

    pub(crate) const fn parameter(&mut self) {
        self.has_parameters = true;
    }

    pub(crate) const fn has_parameters(&self) -> bool {
        self.has_parameters
    }

    pub(crate) fn report(&mut self, problem: AnalysisProblem) {
        self.problems.push(problem);
    }

    pub(crate) fn take_problems(&mut self) -> Vec<AnalysisProblem> {
        self.active = false;
        self.has_parameters = false;
        std::mem::take(&mut self.problems)
    }
}

#[cfg(test)]
mod test_analysis_lifecycle {
    use crate::sem::{AnalysisError, ParseContext};
    use crate::test_helpers::TestParseContext;

    #[test]
    fn completed_analysis_delivers_findings_before_fatal_error() -> testresult::TestResult {
        let mut context = TestParseContext::default();
        context.begin_parameter_type_analysis();
        test_helpers::report_missing_functions(&mut context)?;
        assert_eq!(context.reported_problem_count(), 0);
        let fatal: Result<(), AnalysisError> = Err(std::io::Error::other("analysis failed").into());
        assert_eq!(
            context.finish_analysis(fatal).unwrap_err().to_string(),
            "analysis failed"
        );
        assert_eq!(
            format!("{:?}", context.reported_problems()),
            format!("{:?}", test_helpers::missing_functions())
        );
        Ok(())
    }

    #[test]
    fn diagnostic_rejection_precedes_fatal_result_and_does_not_leak_to_next_run()
    -> testresult::TestResult {
        let mut context = TestParseContext::default();
        context.reject_diagnostics_after(1);
        context.begin_parameter_type_analysis();
        test_helpers::report_missing_functions(&mut context)?;
        let fatal: Result<(), AnalysisError> = Err(std::io::Error::other("later failure").into());
        assert_eq!(
            context.finish_analysis(fatal).unwrap_err().to_string(),
            "diagnostic sink rejected a problem"
        );
        assert_eq!(context.reported_problem_count(), 1);
        context.begin_parameter_type_analysis();
        context.finish_analysis(Ok(()))?;
        assert_eq!(context.reported_problem_count(), 1);
        assert!(!context.parameter_inference_needed());
        Ok(())
    }

    mod test_helpers {
        use crate::StringSpan;
        use crate::sem::{AnalysisError, AnalysisProblem, FullName, ParseContext};
        pub(super) fn missing_functions() -> Vec<AnalysisProblem> {
            vec![
                AnalysisProblem::function_not_found(
                    &FullName::from_local_name("first"),
                    &StringSpan::new(3, 5),
                ),
                AnalysisProblem::function_not_found(
                    &FullName::from_local_name("second"),
                    &StringSpan::new(12, 6),
                ),
            ]
        }
        pub(super) fn report_missing_functions(
            context: &mut impl ParseContext,
        ) -> Result<(), AnalysisError> {
            for problem in missing_functions() {
                context.report_analysis_problem(problem)?;
            }
            Ok(())
        }
    }
}
