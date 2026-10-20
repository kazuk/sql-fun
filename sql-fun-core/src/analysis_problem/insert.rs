use super::DiagnosticMetadata;

/// Structured INSERT violation, independently of its rendered message.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum InsertProblemKind {
    /// Source and destination arity differ.
    Arity,
    /// A PostgreSQL destination column is assigned more than once.
    DuplicateTarget,
    /// A destination view or assigned view column is read-only.
    ReadOnlyTarget,
    /// A writable view does not support RETURNING or ON CONFLICT.
    ViewOperation,
    /// An explicit value targets a generated column.
    GeneratedTarget,
    /// An explicit value violates identity override rules.
    IdentityTarget,
    /// A source type has no assignment conversion to its destination.
    AssignmentType,
    /// A RETURNING qualifier is not visible or is illegal in this dialect.
    ReturningQualifier,
    /// A named constraint is absent or is not an eligible arbiter.
    ConflictConstraint,
    /// An inference target has no eligible unique key.
    ConflictTarget,
    /// A collation or operator class was not found in the applicable namespace.
    ConflictNameMissing,
    /// A collation or operator class has multiple visible matching definitions.
    ConflictNameAmbiguous,
    /// A conflict predicate does not have boolean type.
    ConflictPredicate,
    /// A non-final SQLite UPSERT clause omits its target.
    ConflictChain,
    /// A CTE declares more names than its RETURNING output.
    CteAliases,
}

impl InsertProblemKind {
    /// Diagnostic view operation.
    #[must_use]
    pub const fn view_operation() -> Self {
        Self::ViewOperation
    }
    /// Diagnostic read only target.
    #[must_use]
    pub const fn read_only_target() -> Self {
        Self::ReadOnlyTarget
    }
    /// Diagnostic arity.
    #[must_use]
    pub const fn arity() -> Self {
        Self::Arity
    }
    /// Diagnostic duplicate target.
    #[must_use]
    pub const fn duplicate_target() -> Self {
        Self::DuplicateTarget
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
    /// Diagnostic conflict constraint.
    #[must_use]
    pub const fn conflict_constraint() -> Self {
        Self::ConflictConstraint
    }
    /// Diagnostic conflict target.
    #[must_use]
    pub const fn conflict_target() -> Self {
        Self::ConflictTarget
    }
    /// Diagnostic conflict name missing.
    #[must_use]
    pub const fn conflict_name_missing() -> Self {
        Self::ConflictNameMissing
    }
    /// Diagnostic conflict name ambiguous.
    #[must_use]
    pub const fn conflict_name_ambiguous() -> Self {
        Self::ConflictNameAmbiguous
    }
    /// Diagnostic conflict predicate.
    #[must_use]
    pub const fn conflict_predicate() -> Self {
        Self::ConflictPredicate
    }
    /// Diagnostic conflict chain.
    #[must_use]
    pub const fn conflict_chain() -> Self {
        Self::ConflictChain
    }
    /// Diagnostic cte aliases.
    #[must_use]
    pub const fn cte_aliases() -> Self {
        Self::CteAliases
    }
}

/// A violation of INSERT destination, source or conflict rules.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct InsertProblem {
    kind: InsertProblemKind,
    message: String,
}

impl InsertProblem {
    /// Diagnostic new.
    #[must_use]
    pub fn new(kind: InsertProblemKind, message: &str) -> Self {
        Self {
            kind,
            message: message.to_owned(),
        }
    }

    /// The structured rule violated, independently of diagnostic wording.
    #[must_use]
    pub const fn kind(&self) -> InsertProblemKind {
        self.kind
    }

    /// The INSERT requirement that was violated.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.message())
    }
}
