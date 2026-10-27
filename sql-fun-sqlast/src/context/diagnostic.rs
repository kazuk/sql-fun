use crate::sem::{AnalysisError, AnalysisProblem};

/// Mutable diagnostic sink for semantic analysis.
pub trait DiagnosticContext {
    /// Report a recoverable analysis problem.
    ///
    /// Returns an error if the diagnostic cannot be accepted or rendered.
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError>;
}
