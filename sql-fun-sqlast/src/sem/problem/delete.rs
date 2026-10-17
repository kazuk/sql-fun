use crate::sem::AnalysisError;
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// Structured DELETE violation, independently of its rendered message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteProblemKind {
    /// INDEXED BY does not identify an index of the destination.
    IndexHint,
    /// A relation or column does not accept DELETE.
    ReadOnlyTarget,
    /// A view does not support the requested operation.
    ViewOperation,
    /// A RETURNING source is invalid.
    ReturningQualifier,
    /// A CTE declares too many aliases.
    CteAliases,
    /// WHERE requires boolean type.
    Predicate,
    /// A CURRENT OF portal is missing or statically incompatible with the target.
    Cursor,
}
impl DeleteProblemKind {
    pub(crate) const fn cursor() -> Self {
        Self::Cursor
    }
    pub(crate) const fn index_hint() -> Self {
        Self::IndexHint
    }
    pub(crate) const fn read_only_target() -> Self {
        Self::ReadOnlyTarget
    }
    pub(crate) const fn view_operation() -> Self {
        Self::ViewOperation
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

/// A violation of DELETE destination, predicate or result rules.
#[derive(Clone, Debug)]
pub struct DeleteProblem {
    kind: DeleteProblemKind,
    message: String,
}

impl DeleteProblem {
    pub(crate) fn new(kind: DeleteProblemKind, message: &str) -> Self {
        Self {
            kind,
            message: message.to_owned(),
        }
    }

    pub(super) const fn code(&self) -> &'static str {
        "delete_semantics"
    }

    /// The structured rule violated, independently of diagnostic wording.
    pub const fn kind(&self) -> DeleteProblemKind {
        self.kind
    }

    /// The DELETE requirement that was violated.
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
