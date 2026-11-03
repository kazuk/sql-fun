use std::fmt::Write;

use crate::analysis_problem::{DiagnosticMetadata, DiagnosticSeverity, FullName};

/// Classification of a rejected index alteration or a statistics warning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum IndexProblemKind {
    /// No relation occupies the requested name.
    MissingTarget,
    /// The visible relation is not an index.
    WrongKind,
    /// An option namespace is not accepted by `PostgreSQL`.
    InvalidNamespace,
    /// A storage parameter is not supported by the access method.
    UnknownParameter,
    /// A SET list repeats a parameter.
    DuplicateParameter,
    /// A storage value has the wrong type or range.
    InvalidValue,
    /// A RESET parameter supplies a value.
    ResetValue,
    /// The column ordinal is outside the index layout.
    InvalidColumn,
    /// An ordinary key column cannot have index statistics.
    NonExpressionColumn,
    /// An included column cannot have index statistics.
    IncludedColumn,
    /// A statistics target is below -1.
    StatisticsTooLow,
    /// `PostgreSQL` lowers a statistics target above 10000 to 10000.
    StatisticsClamped,
}
impl IndexProblemKind {
    /// Classify: missing target.
    #[must_use]
    pub const fn missing_target() -> Self {
        Self::MissingTarget
    }
    /// Classify: wrong kind.
    #[must_use]
    pub const fn wrong_kind() -> Self {
        Self::WrongKind
    }
    /// Classify: invalid namespace.
    #[must_use]
    pub const fn invalid_namespace() -> Self {
        Self::InvalidNamespace
    }
    /// Classify: unknown parameter.
    #[must_use]
    pub const fn unknown_parameter() -> Self {
        Self::UnknownParameter
    }
    /// Classify: duplicate parameter.
    #[must_use]
    pub const fn duplicate_parameter() -> Self {
        Self::DuplicateParameter
    }
    /// Classify: invalid value.
    #[must_use]
    pub const fn invalid_value() -> Self {
        Self::InvalidValue
    }
    /// Classify: RESET parameter supplies a value.
    #[must_use]
    pub const fn reset_value() -> Self {
        Self::ResetValue
    }
    /// Classify: invalid column.
    #[must_use]
    pub const fn invalid_column() -> Self {
        Self::InvalidColumn
    }
    /// Classify: non expression column.
    #[must_use]
    pub const fn non_expression_column() -> Self {
        Self::NonExpressionColumn
    }
    /// Classify: included column.
    #[must_use]
    pub const fn included_column() -> Self {
        Self::IncludedColumn
    }
    /// Classify: statistics too low.
    #[must_use]
    pub const fn statistics_too_low() -> Self {
        Self::StatisticsTooLow
    }
    /// Classify: statistics clamped.
    #[must_use]
    pub const fn statistics_clamped() -> Self {
        Self::StatisticsClamped
    }
    /// Diagnostic is warning.
    #[must_use]
    pub const fn is_warning(self) -> bool {
        matches!(self, Self::StatisticsClamped)
    }
    pub(super) const fn message(self) -> &'static str {
        match self {
            Self::MissingTarget => "index does not exist",
            Self::WrongKind => "relation is not an index",
            Self::InvalidNamespace => "unrecognized index parameter namespace",
            Self::UnknownParameter => "unrecognized index storage parameter",
            Self::DuplicateParameter => "index parameter specified more than once",
            Self::InvalidValue => "invalid index storage parameter value",
            Self::ResetValue => "RESET must not include values for parameters",
            Self::InvalidColumn => "index column number is out of range",
            Self::NonExpressionColumn => "cannot alter statistics on a non-expression index column",
            Self::IncludedColumn => "cannot alter statistics on an included index column",
            Self::StatisticsTooLow => "statistics target is below -1",
            Self::StatisticsClamped => "lowering statistics target to 10000",
        }
    }
}

/// Structured index diagnostic, retained independently of diagnostic delivery.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct IndexProblem {
    target: FullName,
    kind: IndexProblemKind,
    parameter: Option<String>,
    value: Option<String>,
    column: Option<i32>,
}
impl IndexProblem {
    /// Diagnostic target.
    #[must_use]
    pub fn target(target: impl Into<FullName>, kind: IndexProblemKind) -> Self {
        Self {
            target: target.into(),
            kind,
            parameter: None,
            value: None,
            column: None,
        }
    }
    /// Diagnostic parameter.
    #[must_use]
    pub fn parameter(
        target: impl Into<FullName>,
        kind: IndexProblemKind,
        name: &str,
        value: Option<&str>,
    ) -> Self {
        let mut problem = Self::target(target, kind);
        problem.parameter = Some(name.to_owned());
        problem.value = value.map(str::to_owned);
        problem
    }
    /// Diagnostic statistics.
    #[must_use]
    pub fn statistics(
        target: impl Into<FullName>,
        kind: IndexProblemKind,
        column: i32,
        value: i32,
    ) -> Self {
        let mut problem = Self::target(target, kind);
        problem.column = Some(column);
        problem.value = Some(value.to_string());
        problem
    }
    /// Index or visible relation identity associated with this diagnostic.
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.target
    }
    /// Semantic diagnostic classification.
    #[must_use]
    pub const fn kind(&self) -> IndexProblemKind {
        self.kind
    }
    /// Rejected storage parameter, including its namespace when specified.
    #[must_use]
    pub fn parameter_name(&self) -> Option<&str> {
        self.parameter.as_deref()
    }
    /// Requested value, before validation or clamping.
    #[must_use]
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
    /// Requested one-based index column ordinal.
    #[must_use]
    pub const fn column(&self) -> Option<i32> {
        self.column
    }
    /// Whether this diagnostic permits application.
    #[must_use]
    pub const fn is_warning(&self) -> bool {
        self.kind.is_warning()
    }
    /// Diagnostic code.
    pub(super) const fn code(&self) -> &'static str {
        if self.is_warning() {
            "index_statistics_clamped"
        } else {
            "index_invalid_alteration"
        }
    }
    /// Diagnostic severity.
    pub(super) const fn severity(&self) -> DiagnosticSeverity {
        if self.is_warning() {
            DiagnosticSeverity::Warning
        } else {
            DiagnosticSeverity::Error
        }
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        let mut message = format!("{}: {}", self.target, self.kind.message());
        if let Some(parameter) = &self.parameter {
            let _ = write!(message, " ({parameter})");
        }
        if let Some(column) = self.column {
            let _ = write!(message, " (column {column})");
        }
        if let Some(value) = &self.value {
            let _ = write!(message, ": {value}");
        }
        DiagnosticMetadata::from(message)
    }
}
