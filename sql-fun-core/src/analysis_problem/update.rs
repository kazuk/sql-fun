use crate::analysis_problem::DiagnosticMetadata;

/// Structured UPDATE violation, independently of its rendered message.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum UpdateProblemKind {
    /// INDEXED BY does not identify an index of the destination.
    IndexHint,
    /// DEFAULT targets an element or subfield instead of a whole column.
    DefaultTarget,
    /// A column has overlapping assignments.
    DuplicateTarget,
    /// A relation or column does not accept UPDATE.
    ReadOnlyTarget,
    /// A view does not support the requested operation.
    ViewOperation,
    /// An assignment violates generated-column rules.
    GeneratedTarget,
    /// An assignment violates identity-column rules.
    IdentityTarget,
    /// The RHS has no assignment conversion.
    AssignmentType,
    /// A RETURNING source is invalid.
    ReturningQualifier,
    /// A CTE declares too many aliases.
    CteAliases,
    /// WHERE requires boolean type.
    Predicate,
    /// A CURRENT OF portal is missing or statically incompatible with the target.
    Cursor,
}
impl UpdateProblemKind {
    /// Diagnostic cursor.
    #[must_use]
    pub const fn cursor() -> Self {
        Self::Cursor
    }
    /// Diagnostic default target.
    #[must_use]
    pub const fn default_target() -> Self {
        Self::DefaultTarget
    }
    /// Diagnostic index hint.
    #[must_use]
    pub const fn index_hint() -> Self {
        Self::IndexHint
    }
    /// Diagnostic duplicate target.
    #[must_use]
    pub const fn duplicate_target() -> Self {
        Self::DuplicateTarget
    }
    /// Diagnostic read only target.
    #[must_use]
    pub const fn read_only_target() -> Self {
        Self::ReadOnlyTarget
    }
    /// Diagnostic view operation.
    #[must_use]
    pub const fn view_operation() -> Self {
        Self::ViewOperation
    }
    /// Diagnostic generated target.
    #[must_use]
    pub const fn generated_target() -> Self {
        Self::GeneratedTarget
    }
    /// Diagnostic identity target.
    #[must_use]
    pub const fn identity_target() -> Self {
        Self::IdentityTarget
    }
    /// Diagnostic assignment type.
    #[must_use]
    pub const fn assignment_type() -> Self {
        Self::AssignmentType
    }
    /// Diagnostic returning qualifier.
    #[must_use]
    pub const fn returning_qualifier() -> Self {
        Self::ReturningQualifier
    }
    /// Diagnostic cte aliases.
    #[must_use]
    pub const fn cte_aliases() -> Self {
        Self::CteAliases
    }
    /// Diagnostic predicate.
    #[must_use]
    pub const fn predicate() -> Self {
        Self::Predicate
    }
}

/// A violation of UPDATE destination, assignment or result rules.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct UpdateProblem {
    kind: UpdateProblemKind,
    message: String,
}

impl UpdateProblem {
    /// Diagnostic new.
    #[must_use]
    pub fn new(kind: UpdateProblemKind, message: &str) -> Self {
        Self {
            kind,
            message: message.to_owned(),
        }
    }

    /// The structured rule violated, independently of diagnostic wording.
    #[must_use]
    pub const fn kind(&self) -> UpdateProblemKind {
        self.kind
    }

    /// The UPDATE requirement that was violated.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.message())
    }
}
