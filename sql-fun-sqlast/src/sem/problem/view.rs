use crate::sem::{AnalysisError, FullName};
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// A view declaration or capability cannot be analyzed as requested.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ViewProblem {
    /// An existing View could not be evaluated; the original cause remains in AnalysisError.
    EvaluationFailed(FullName, String),
    /// The declaration target is absent or is not an ordinary view.
    InvalidTarget(FullName),
    /// A view default refers to an unknown column.
    UnknownDefaultColumn(FullName, String),
    /// Explicitly loaded catalog data lacks required write facts.
    MissingMetadata(FullName),
    /// PostgreSQL does not allow this ALTER operation on a view.
    InvalidAlter,
}
impl ViewProblem {
    /// Exact catalog identity whose required semantic definition is absent.
    pub const fn missing_metadata_view(&self) -> Option<&FullName> {
        if let Self::MissingMetadata(view) = self {
            Some(view)
        } else {
            None
        }
    }

    pub(crate) fn evaluation_failed(view: &FullName, cause: &AnalysisError) -> Self {
        Self::EvaluationFailed(view.clone(), cause.to_string())
    }
    pub(crate) fn invalid_target(view: &FullName) -> Self {
        Self::InvalidTarget(view.clone())
    }
    pub(crate) fn unknown_default_column(view: &FullName, name: &str) -> Self {
        Self::UnknownDefaultColumn(view.clone(), name.to_owned())
    }
    pub(crate) const fn invalid_alter() -> Self {
        Self::InvalidAlter
    }

    pub(crate) fn missing_metadata(view: &FullName) -> Self {
        Self::MissingMetadata(view.clone())
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::EvaluationFailed(..) => "view_evaluation_failed",
            Self::InvalidTarget(_) => "view_invalid_target",
            Self::UnknownDefaultColumn(..) => "view_unknown_default_column",
            Self::MissingMetadata(_) => "view_missing_write_metadata",
            Self::InvalidAlter => "view_invalid_alter",
        }
    }
    pub(super) fn render_report(
        &self,
        handler: &GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let message = match self {
            Self::EvaluationFailed(view, cause) => format!("cannot evaluate View {view}: {cause}"),
            Self::InvalidTarget(view) => {
                format!("declaration target {view} is not an ordinary view")
            }
            Self::UnknownDefaultColumn(view, column) => {
                format!("default column {column} does not exist on {view}")
            }
            Self::MissingMetadata(view) => format!(
                "the semantic definition required for {view} is missing; reload the complete schema declarations"
            ),
            Self::InvalidAlter => {
                "PostgreSQL does not permit ALTER trigger/rule state on views".to_owned()
            }
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
