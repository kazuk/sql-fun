use crate::sem::{AnalysisError, FullName};
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// A view declaration or capability cannot be analyzed as requested.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ViewProblem {
    /// The declaration target is absent or is not an ordinary view.
    InvalidTarget(FullName),
    /// A declaration repeats an existing object.
    DuplicateObject(FullName, String),
    /// A declaration refers to a missing trigger or rule.
    MissingObject(FullName, String),
    /// A view default refers to an unknown column.
    UnknownDefaultColumn(FullName, String),
    /// Replacement removes, renames or changes existing columns.
    InvalidReplacement(FullName),
    /// Explicitly loaded catalog data lacks required write facts.
    MissingMetadata(FullName),
    /// PostgreSQL does not allow this ALTER operation on a view.
    InvalidAlter,
}
impl ViewProblem {
    pub(crate) fn invalid_target(view: &FullName) -> Self {
        Self::InvalidTarget(view.clone())
    }
    pub(crate) fn duplicate_object(view: &FullName, name: &str) -> Self {
        Self::DuplicateObject(view.clone(), name.to_owned())
    }
    pub(crate) fn missing_object(view: &FullName, name: &str) -> Self {
        Self::MissingObject(view.clone(), name.to_owned())
    }
    pub(crate) fn unknown_default_column(view: &FullName, name: &str) -> Self {
        Self::UnknownDefaultColumn(view.clone(), name.to_owned())
    }
    pub(crate) fn invalid_replacement(view: &FullName) -> Self {
        Self::InvalidReplacement(view.clone())
    }
    pub(crate) const fn invalid_alter() -> Self {
        Self::InvalidAlter
    }

    pub(crate) fn missing_metadata(view: &FullName) -> Self {
        Self::MissingMetadata(view.clone())
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::InvalidTarget(_) => "view_invalid_target",
            Self::DuplicateObject(..) => "view_duplicate_object",
            Self::MissingObject(..) => "view_missing_object",
            Self::UnknownDefaultColumn(..) => "view_unknown_default_column",
            Self::InvalidReplacement(_) => "view_invalid_replacement",
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
            Self::InvalidTarget(view) => {
                format!("declaration target {view} is not an ordinary view")
            }
            Self::DuplicateObject(view, name) => format!("object {name} already exists on {view}"),
            Self::MissingObject(view, name) => format!("object {name} does not exist on {view}"),
            Self::UnknownDefaultColumn(view, column) => {
                format!("default column {column} does not exist on {view}")
            }
            Self::InvalidReplacement(view) => {
                format!("replacement of {view} cannot remove, rename or change existing columns")
            }
            Self::MissingMetadata(view) => format!(
                "write metadata is missing for {view}; reload the complete schema declarations"
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
