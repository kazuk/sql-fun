use super::{DiagnosticMetadata, FullName};

/// A view declaration or capability cannot be analyzed as requested.
#[derive(Debug, Clone, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum ViewProblem {
    /// The declaration target is absent or is not an ordinary view.
    InvalidTarget(FullName),
    /// A view default refers to an unknown column.
    UnknownDefaultColumn(FullName, String),
    /// Explicitly loaded catalog data lacks required write facts.
    MissingMetadata(FullName),
    /// `PostgreSQL` does not allow this ALTER operation on a view.
    InvalidAlter,
}
impl ViewProblem {
    /// Exact catalog identity whose required semantic definition is absent.
    #[must_use]
    pub const fn missing_metadata_view(&self) -> Option<&FullName> {
        if let Self::MissingMetadata(view) = self {
            Some(view)
        } else {
            None
        }
    }

    /// Diagnostic invalid target.
    #[must_use]
    pub fn invalid_target(view: impl Into<FullName>) -> Self {
        Self::InvalidTarget(view.into())
    }
    /// Diagnostic unknown default column.
    #[must_use]
    pub fn unknown_default_column(view: impl Into<FullName>, name: &str) -> Self {
        Self::UnknownDefaultColumn(view.into(), name.to_owned())
    }
    /// Diagnostic invalid alter.
    #[must_use]
    pub const fn invalid_alter() -> Self {
        Self::InvalidAlter
    }

    /// Diagnostic missing metadata.
    #[must_use]
    pub fn missing_metadata(view: impl Into<FullName>) -> Self {
        Self::MissingMetadata(view.into())
    }
    /// Diagnostic code.
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::InvalidTarget(_) => "view_invalid_target",
            Self::UnknownDefaultColumn(..) => "view_unknown_default_column",
            Self::MissingMetadata(_) => "view_missing_write_metadata",
            Self::InvalidAlter => "view_invalid_alter",
        }
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        let message = match self {
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
        DiagnosticMetadata::from(message)
    }
}
