use crate::sem::AnalysisError;
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// Invocation-shape violations decidable without a function signature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallArgumentProblem {
    /// A supplied argument name occurs more than once.
    DuplicateName(String),
    /// A positional argument follows a named argument (zero-based call position).
    PositionalAfterNamed(usize),
}
impl CallArgumentProblem {
    pub(crate) fn duplicate_name(name: &str) -> Self {
        Self::DuplicateName(name.to_owned())
    }
    pub(crate) const fn positional_after_named(position: usize) -> Self {
        Self::PositionalAfterNamed(position)
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::DuplicateName(_) => "call_argument_duplicate_name",
            Self::PositionalAfterNamed(_) => "call_argument_positional_after_named",
        }
    }
    pub(super) fn render_report(
        &self,
        handler: &GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let message = match self {
            Self::DuplicateName(name) => format!("argument name \"{name}\" used more than once"),
            Self::PositionalAfterNamed(position) => format!(
                "positional argument at call position {position} cannot follow named argument"
            ),
        };
        let diagnostic = MietteDiagnostic::new(message)
            .with_code(self.code())
            .with_severity(Severity::Error);
        let report = Report::new(diagnostic).with_source_code(NamedSource::new("SQL", source));
        let mut rendered = String::new();
        handler.render_report(&mut rendered, &*report)?;
        Ok(rendered)
    }
}
