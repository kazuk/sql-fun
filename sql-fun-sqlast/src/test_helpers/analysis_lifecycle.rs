use crate::StringSpan;
use crate::context::ParseContext;
use crate::names::FullName;
use crate::sem::{AnalysisError, AnalysisProblem};
pub(crate) fn missing_functions() -> Vec<AnalysisProblem> {
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
pub(crate) fn report_missing_functions(
    context: &mut impl ParseContext,
) -> Result<(), AnalysisError> {
    for problem in missing_functions() {
        context.report_analysis_problem(problem)?;
    }
    Ok(())
}
