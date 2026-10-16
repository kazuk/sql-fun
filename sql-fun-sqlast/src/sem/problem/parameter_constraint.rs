use crate::sem::AnalysisError;
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// Recoverable inconsistency between SQL-derived parameter requirements.
#[derive(Clone, Debug)]
pub struct ParameterConstraintProblem {
    message: String,
}

impl ParameterConstraintProblem {
    pub(crate) fn new(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }

    pub(super) const fn code(&self) -> &'static str {
        "parameter_constraint"
    }

    /// Requirements or parameter uses that could not be reconciled.
    pub fn message(&self) -> &str {
        &self.message
    }

    pub(super) fn render_report(
        &self,
        handler: &GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let diagnostic = MietteDiagnostic::new(self.message.clone())
            .with_code(self.code())
            .with_severity(Severity::Error);
        let report = Report::new(diagnostic).with_source_code(NamedSource::new("SQL", source));
        let mut rendered = String::new();
        handler.render_report(&mut rendered, &*report)?;
        Ok(rendered)
    }
}
