use crate::analysis_problem::{DiagnosticMessage, DiagnosticMetadata, FullName, Kind};

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
    /// A dependent view prevents DROP VIEW.
    DropDependency(FullName),
    /// A DROP VIEW name resolves to a different kind of relation.
    DropWrongKind(FullName, Kind),
    /// An extension owns a view selected for deletion.
    DropExtensionMember(FullName),
    /// DROP VIEW referenced another database.
    DropCrossDatabase(FullName),
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
    /// Diagnostic for a dependent view that blocks DROP VIEW.
    #[must_use]
    pub fn drop_dependency(name: impl Into<FullName>) -> Self {
        Self::DropDependency(name.into())
    }

    /// Diagnostic for a DROP VIEW target of an incompatible kind.
    #[must_use]
    pub fn drop_wrong_kind(name: impl Into<FullName>, kind: impl Into<Kind>) -> Self {
        Self::DropWrongKind(name.into(), kind.into())
    }

    /// Diagnostic for a DROP VIEW target required by its owning extension.
    #[must_use]
    pub fn drop_extension_member(name: impl Into<FullName>) -> Self {
        Self::DropExtensionMember(name.into())
    }

    /// Diagnostic for a DROP VIEW target outside the current database.
    #[must_use]
    pub fn drop_cross_database(name: impl Into<FullName>) -> Self {
        Self::DropCrossDatabase(name.into())
    }

    /// Diagnostic code.
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::InvalidTarget(_) => "view_invalid_target",
            Self::UnknownDefaultColumn(..) => "view_unknown_default_column",
            Self::MissingMetadata(_) => "view_missing_write_metadata",
            Self::InvalidAlter => "view_invalid_alter",
            Self::DropDependency(_) => "drop_view_dependency",
            Self::DropWrongKind(..) => "drop_view_wrong_kind",
            Self::DropExtensionMember(_) => "drop_view_extension_member",
            Self::DropCrossDatabase(_) => "cross_database_drop_view",
        }
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}
