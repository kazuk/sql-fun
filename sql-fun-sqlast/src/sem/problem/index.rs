use crate::sem::{AnalysisError, FullName};

/// Classification of a rejected index alteration or a statistics warning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexProblemKind {
    /// No relation occupies the requested name.
    MissingTarget,
    /// The visible relation is not an index.
    WrongKind,
    /// An option namespace is not accepted by PostgreSQL.
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
    /// PostgreSQL lowers a statistics target above 10000 to 10000.
    StatisticsClamped,
}
impl IndexProblemKind {
    /// Classify: missing target.
    pub const fn missing_target() -> Self {
        Self::MissingTarget
    }
    /// Classify: wrong kind.
    pub const fn wrong_kind() -> Self {
        Self::WrongKind
    }
    /// Classify: invalid namespace.
    pub const fn invalid_namespace() -> Self {
        Self::InvalidNamespace
    }
    /// Classify: unknown parameter.
    pub const fn unknown_parameter() -> Self {
        Self::UnknownParameter
    }
    /// Classify: duplicate parameter.
    pub const fn duplicate_parameter() -> Self {
        Self::DuplicateParameter
    }
    /// Classify: invalid value.
    pub const fn invalid_value() -> Self {
        Self::InvalidValue
    }
    /// Classify: RESET parameter supplies a value.
    pub const fn reset_value() -> Self {
        Self::ResetValue
    }
    /// Classify: invalid column.
    pub const fn invalid_column() -> Self {
        Self::InvalidColumn
    }
    /// Classify: non expression column.
    pub const fn non_expression_column() -> Self {
        Self::NonExpressionColumn
    }
    /// Classify: included column.
    pub const fn included_column() -> Self {
        Self::IncludedColumn
    }
    /// Classify: statistics too low.
    pub const fn statistics_too_low() -> Self {
        Self::StatisticsTooLow
    }
    /// Classify: statistics clamped.
    pub const fn statistics_clamped() -> Self {
        Self::StatisticsClamped
    }
    pub(crate) const fn is_warning(self) -> bool {
        matches!(self, Self::StatisticsClamped)
    }
    const fn message(self) -> &'static str {
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
#[derive(Clone, Debug)]
pub struct IndexProblem {
    target: FullName,
    kind: IndexProblemKind,
    parameter: Option<String>,
    value: Option<String>,
    column: Option<i32>,
}
impl IndexProblem {
    pub(crate) fn target(target: &FullName, kind: IndexProblemKind) -> Self {
        Self {
            target: target.clone(),
            kind,
            parameter: None,
            value: None,
            column: None,
        }
    }
    pub(crate) fn parameter(
        target: &FullName,
        kind: IndexProblemKind,
        name: &str,
        value: Option<&str>,
    ) -> Self {
        let mut problem = Self::target(target, kind);
        problem.parameter = Some(name.to_owned());
        problem.value = value.map(str::to_owned);
        problem
    }
    pub(crate) fn statistics(
        target: &FullName,
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
    pub const fn name(&self) -> &FullName {
        &self.target
    }
    /// Semantic diagnostic classification.
    pub const fn kind(&self) -> IndexProblemKind {
        self.kind
    }
    /// Rejected storage parameter, including its namespace when specified.
    pub fn parameter_name(&self) -> Option<&str> {
        self.parameter.as_deref()
    }
    /// Requested value, before validation or clamping.
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
    /// Requested one-based index column ordinal.
    pub const fn column(&self) -> Option<i32> {
        self.column
    }
    /// Whether this diagnostic permits application.
    pub const fn is_warning(&self) -> bool {
        self.kind.is_warning()
    }
    pub(super) const fn code(&self) -> &'static str {
        if self.is_warning() {
            "index_statistics_clamped"
        } else {
            "index_invalid_alteration"
        }
    }
    pub(super) const fn severity(&self) -> ::miette::Severity {
        if self.is_warning() {
            ::miette::Severity::Warning
        } else {
            ::miette::Severity::Error
        }
    }
    pub(super) fn render_report(
        &self,
        handler: &::miette::GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let severity = self.severity();
        let mut message = format!("{}: {}", self.target, self.kind.message());
        if let Some(parameter) = &self.parameter {
            message.push_str(&format!(" ({parameter})"));
        }
        if let Some(column) = self.column {
            message.push_str(&format!(" (column {column})"));
        }
        if let Some(value) = &self.value {
            message.push_str(&format!(": {value}"));
        }
        let diagnostic = ::miette::MietteDiagnostic::new(message)
            .with_code(self.code())
            .with_severity(severity);
        let report = ::miette::Report::new(diagnostic)
            .with_source_code(::miette::NamedSource::new("SQL", source));
        let mut output = String::new();
        handler.render_report(&mut output, &*report)?;
        Ok(output)
    }
}
