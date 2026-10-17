use crate::sem::AnalysisError;
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// Structured INSERT violation, independently of its rendered message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    pub(crate) const fn view_operation() -> Self {
        Self::ViewOperation
    }
    pub(crate) const fn read_only_target() -> Self {
        Self::ReadOnlyTarget
    }
    pub(crate) const fn arity() -> Self {
        Self::Arity
    }
    pub(crate) const fn duplicate_target() -> Self {
        Self::DuplicateTarget
    }
    pub(crate) const fn generated_target() -> Self {
        Self::GeneratedTarget
    }
    pub(crate) const fn identity_target() -> Self {
        Self::IdentityTarget
    }
    pub(crate) const fn assignment_type() -> Self {
        Self::AssignmentType
    }
    pub(crate) const fn returning_qualifier() -> Self {
        Self::ReturningQualifier
    }
    pub(crate) const fn conflict_constraint() -> Self {
        Self::ConflictConstraint
    }
    pub(crate) const fn conflict_target() -> Self {
        Self::ConflictTarget
    }
    pub(crate) const fn conflict_name_missing() -> Self {
        Self::ConflictNameMissing
    }
    pub(crate) const fn conflict_name_ambiguous() -> Self {
        Self::ConflictNameAmbiguous
    }
    pub(crate) const fn conflict_predicate() -> Self {
        Self::ConflictPredicate
    }
    pub(crate) const fn conflict_chain() -> Self {
        Self::ConflictChain
    }
    pub(crate) const fn cte_aliases() -> Self {
        Self::CteAliases
    }
}

/// A violation of INSERT destination, source or conflict rules.
#[derive(Clone, Debug)]
pub struct InsertProblem {
    kind: InsertProblemKind,
    message: String,
}

impl InsertProblem {
    pub(crate) fn new(kind: InsertProblemKind, message: &str) -> Self {
        Self {
            kind,
            message: message.to_owned(),
        }
    }

    pub(super) const fn code(&self) -> &'static str {
        "insert_semantics"
    }

    /// The structured rule violated, independently of diagnostic wording.
    pub const fn kind(&self) -> InsertProblemKind {
        self.kind
    }

    /// The INSERT requirement that was violated.
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
