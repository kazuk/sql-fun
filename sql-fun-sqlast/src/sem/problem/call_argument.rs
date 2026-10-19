use crate::{StringSpan, sem::AnalysisError};
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// Invocation-shape violations decidable without a function signature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallArgumentProblem {
    /// A supplied argument name occurs more than once.
    DuplicateName(String, Option<StringSpan>),
    /// A positional argument follows a named argument (zero-based call position).
    PositionalAfterNamed(usize, Option<StringSpan>),
}
impl CallArgumentProblem {
    pub(crate) fn duplicate_name(name: &str, span: &Option<StringSpan>) -> Self {
        Self::DuplicateName(name.to_owned(), *span)
    }
    pub(crate) const fn positional_after_named(position: usize, span: &Option<StringSpan>) -> Self {
        Self::PositionalAfterNamed(position, *span)
    }
    pub(super) const fn statement_relative_span(&self) -> Option<StringSpan> {
        match self {
            Self::DuplicateName(_, span) | Self::PositionalAfterNamed(_, span) => *span,
        }
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::DuplicateName(..) => "call_argument_duplicate_name",
            Self::PositionalAfterNamed(..) => "call_argument_positional_after_named",
        }
    }
    pub(super) fn render_report(
        &self,
        handler: &GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let message = match self {
            Self::DuplicateName(name, _) => format!("argument name \"{name}\" used more than once"),
            Self::PositionalAfterNamed(position, _) => format!(
                "positional argument at call position {position} cannot follow named argument"
            ),
        };
        let mut diagnostic = MietteDiagnostic::new(message)
            .with_code(self.code())
            .with_severity(Severity::Error);
        if let Some(span) = self.statement_relative_span() {
            diagnostic = diagnostic.with_labels([::miette::LabeledSpan::new_with_span(
                Some("invalid call argument".to_owned()),
                (span.offset(), span.len()),
            )]);
        }
        let report = Report::new(diagnostic).with_source_code(NamedSource::new("SQL", source));
        let mut rendered = String::new();
        handler.render_report(&mut rendered, &*report)?;
        Ok(rendered)
    }
}

#[cfg(test)]
mod test_diagnostics {
    use super::CallArgumentProblem;
    use crate::StringSpan;

    #[test]
    fn renders_the_retained_argument_location() {
        let problem = CallArgumentProblem::duplicate_name("a", &Some(StringSpan::new(24, 1)));
        let report = problem
            .render_report(
                &::miette::GraphicalReportHandler::new(),
                "SELECT identity(a => 1,  a => 2)".to_owned(),
            )
            .unwrap();
        assert!(report.contains(problem.code()));
        assert!(report.contains("invalid call argument"));
        assert!(report.contains("SELECT identity"));
    }
}
