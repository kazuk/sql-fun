use crate::sem::AnalysisError;
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// Structured DELETE violation, independently of its rendered message.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// The named cursor is absent from the current session.
    MissingCursor {
        /// Normalized portal name.
        name: String,
    },
    /// The named cursor is statically non-updatable.
    NonUpdatableCursor {
        /// Normalized portal name.
        name: String,
    },
    /// The cursor targets another relation.
    CursorTargetMismatch {
        /// Normalized portal name.
        name: String,
        /// Requested DELETE destination.
        expected: crate::sem::TableName,
        /// Cursor's established target.
        actual: crate::sem::TableName,
    },
    /// A procedural cursor parameter needs an unavailable variable resolver.
    CursorParameter(i32),
    /// DELETE is not permitted as a SQLite CTE body.
    SqliteCte,
    /// Reading a DELETE CTE requires RETURNING.
    UnreadableCte(String),
}
impl DeleteProblemKind {
    pub(crate) fn missing_cursor(name: &str) -> Self {
        Self::MissingCursor {
            name: name.to_owned(),
        }
    }
    pub(crate) fn non_updatable_cursor(name: &str) -> Self {
        Self::NonUpdatableCursor {
            name: name.to_owned(),
        }
    }
    pub(crate) fn cursor_target_mismatch(
        name: &str,
        expected: &crate::sem::TableName,
        actual: &crate::sem::TableName,
    ) -> Self {
        Self::CursorTargetMismatch {
            name: name.to_owned(),
            expected: expected.clone(),
            actual: actual.clone(),
        }
    }
    pub(crate) const fn cursor_parameter(number: i32) -> Self {
        Self::CursorParameter(number)
    }
    pub(crate) const fn sqlite_cte() -> Self {
        Self::SqliteCte
    }
    pub(crate) fn unreadable_cte(name: &str) -> Self {
        Self::UnreadableCte(name.to_owned())
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
}

impl DeleteProblem {
    pub(crate) const fn new(kind: DeleteProblemKind) -> Self {
        Self { kind }
    }

    pub(super) const fn code(&self) -> &'static str {
        "delete_semantics"
    }

    /// The structured rule violated, independently of diagnostic wording.
    pub const fn kind(&self) -> &DeleteProblemKind {
        &self.kind
    }

    /// The DELETE requirement that was violated.
    pub fn message(&self) -> String {
        match &self.kind {
            DeleteProblemKind::MissingCursor { name } => {
                format!("cursor {name:?} does not exist in the current session scope")
            }
            DeleteProblemKind::NonUpdatableCursor { name } => {
                format!("cursor {name:?} is not statically updatable")
            }
            DeleteProblemKind::CursorTargetMismatch {
                name,
                expected,
                actual,
            } => format!("cursor {name:?} targets {actual}, not DELETE destination {expected}"),
            DeleteProblemKind::CursorParameter(number) => {
                format!("CURRENT OF parameter {number} requires a procedural variable resolver")
            }
            DeleteProblemKind::UnreadableCte(name) => {
                format!("DELETE CTE {name} requires RETURNING to be read")
            }
            DeleteProblemKind::IndexHint => {
                "INDEXED BY does not identify an index of the DELETE destination".to_owned()
            }
            DeleteProblemKind::ReadOnlyTarget => "destination does not support DELETE".to_owned(),
            DeleteProblemKind::ViewOperation => "view does not support DELETE RETURNING".to_owned(),
            DeleteProblemKind::ReturningQualifier => {
                "RETURNING star source is invalid or not visible in this dialect".to_owned()
            }
            DeleteProblemKind::CteAliases => "DELETE CTE has more aliases than outputs".to_owned(),
            DeleteProblemKind::Predicate => "DELETE WHERE requires boolean type".to_owned(),
            DeleteProblemKind::SqliteCte => "SQLite DELETE CTE is unsupported".to_owned(),
        }
    }

    pub(super) fn render_report(
        &self,
        handler: &GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let diagnostic = MietteDiagnostic::new(self.message())
            .with_code(self.code())
            .with_severity(Severity::Error);
        let report = Report::new(diagnostic).with_source_code(NamedSource::new("SQL", source));
        let mut rendered = String::new();
        handler.render_report(&mut rendered, &*report)?;
        Ok(rendered)
    }
}
