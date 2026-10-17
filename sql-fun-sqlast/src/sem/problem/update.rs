use crate::sem::AnalysisError;
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// Structured UPDATE violation, independently of its rendered message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    pub(crate) const fn cursor() -> Self {
        Self::Cursor
    }
    pub(crate) const fn default_target() -> Self {
        Self::DefaultTarget
    }
    pub(crate) const fn index_hint() -> Self {
        Self::IndexHint
    }
    pub(crate) const fn duplicate_target() -> Self {
        Self::DuplicateTarget
    }
    pub(crate) const fn read_only_target() -> Self {
        Self::ReadOnlyTarget
    }
    pub(crate) const fn view_operation() -> Self {
        Self::ViewOperation
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
    pub(crate) const fn cte_aliases() -> Self {
        Self::CteAliases
    }
    pub(crate) const fn predicate() -> Self {
        Self::Predicate
    }
}

/// A violation of UPDATE destination, assignment or result rules.
#[derive(Clone, Debug)]
pub struct UpdateProblem {
    kind: UpdateProblemKind,
    message: String,
}

impl UpdateProblem {
    pub(crate) fn new(kind: UpdateProblemKind, message: &str) -> Self {
        Self {
            kind,
            message: message.to_owned(),
        }
    }

    pub(super) const fn code(&self) -> &'static str {
        "update_semantics"
    }

    /// The structured rule violated, independently of diagnostic wording.
    pub const fn kind(&self) -> UpdateProblemKind {
        self.kind
    }

    /// The UPDATE requirement that was violated.
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
