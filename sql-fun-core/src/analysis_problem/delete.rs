use super::{DiagnosticMetadata, TableName};

/// A normalized portal name retained by a cursor diagnostic.
#[derive(Clone, Debug, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct CursorNameProblem {
    name: String,
}
impl CursorNameProblem {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
        }
    }
    fn missing_message(&self) -> String {
        format!(
            "cursor {:?} does not exist in the current session scope",
            self.name
        )
    }
    fn non_updatable_message(&self) -> String {
        format!("cursor {:?} is not statically updatable", self.name)
    }
}

/// The requested DELETE destination and a cursor's established target.
#[derive(Clone, Debug, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct CursorTargetMismatchProblem {
    name: String,
    expected: TableName,
    actual: TableName,
}
impl CursorTargetMismatchProblem {
    fn new(name: &str, expected: impl Into<TableName>, actual: impl Into<TableName>) -> Self {
        Self {
            name: name.to_owned(),
            expected: expected.into(),
            actual: actual.into(),
        }
    }
    fn message(&self) -> String {
        format!(
            "cursor {:?} targets {}, not DELETE destination {}",
            self.name, self.actual, self.expected
        )
    }
}

/// Structured DELETE violation, independently of its rendered message.
#[derive(Clone, Debug, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
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
    MissingCursor(CursorNameProblem),
    /// The named cursor is statically non-updatable.
    NonUpdatableCursor(CursorNameProblem),
    /// The cursor targets another relation.
    CursorTargetMismatch(CursorTargetMismatchProblem),
    /// A procedural cursor parameter needs an unavailable variable resolver.
    CursorParameter(i32),
    /// DELETE is not permitted as a `SQLite` CTE body.
    SqliteCte,
    /// Reading a DELETE CTE requires RETURNING.
    UnreadableCte(String),
}
impl DeleteProblemKind {
    /// Diagnostic missing cursor.
    #[must_use]
    pub fn missing_cursor(name: &str) -> Self {
        Self::MissingCursor(CursorNameProblem::new(name))
    }
    /// Diagnostic non updatable cursor.
    #[must_use]
    pub fn non_updatable_cursor(name: &str) -> Self {
        Self::NonUpdatableCursor(CursorNameProblem::new(name))
    }
    /// Diagnostic cursor target mismatch.
    #[must_use]
    pub fn cursor_target_mismatch(
        name: &str,
        expected: impl Into<TableName>,
        actual: impl Into<TableName>,
    ) -> Self {
        Self::CursorTargetMismatch(CursorTargetMismatchProblem::new(name, expected, actual))
    }
    /// Diagnostic cursor parameter.
    #[must_use]
    pub const fn cursor_parameter(number: i32) -> Self {
        Self::CursorParameter(number)
    }
    /// Diagnostic sqlite cte.
    #[must_use]
    pub const fn sqlite_cte() -> Self {
        Self::SqliteCte
    }
    /// Diagnostic unreadable cte.
    #[must_use]
    pub fn unreadable_cte(name: &str) -> Self {
        Self::UnreadableCte(name.to_owned())
    }
    /// Diagnostic index hint.
    #[must_use]
    pub const fn index_hint() -> Self {
        Self::IndexHint
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
    fn message(&self) -> String {
        match self {
            Self::MissingCursor(problem) => problem.missing_message(),
            Self::NonUpdatableCursor(problem) => problem.non_updatable_message(),
            Self::CursorTargetMismatch(problem) => problem.message(),
            Self::CursorParameter(number) => {
                format!("CURRENT OF parameter {number} requires a procedural variable resolver")
            }
            Self::UnreadableCte(name) => {
                format!("DELETE CTE {name} requires RETURNING to be read")
            }
            Self::IndexHint => {
                "INDEXED BY does not identify an index of the DELETE destination".to_owned()
            }
            Self::ReadOnlyTarget => "destination does not support DELETE".to_owned(),
            Self::ViewOperation => "view does not support DELETE RETURNING".to_owned(),
            Self::ReturningQualifier => {
                "RETURNING star source is invalid or not visible in this dialect".to_owned()
            }
            Self::CteAliases => "DELETE CTE has more aliases than outputs".to_owned(),
            Self::Predicate => "DELETE WHERE requires boolean type".to_owned(),
            Self::SqliteCte => "SQLite DELETE CTE is unsupported".to_owned(),
        }
    }
}

/// A violation of DELETE destination, predicate or result rules.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct DeleteProblem {
    kind: DeleteProblemKind,
}

impl DeleteProblem {
    /// Diagnostic new.
    #[must_use]
    pub const fn new(kind: DeleteProblemKind) -> Self {
        Self { kind }
    }

    /// The structured rule violated, independently of diagnostic wording.
    #[must_use]
    pub const fn kind(&self) -> &DeleteProblemKind {
        &self.kind
    }

    /// The DELETE requirement that was violated.
    #[must_use]
    pub fn message(&self) -> String {
        self.kind.message()
    }

    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.message())
    }
}
