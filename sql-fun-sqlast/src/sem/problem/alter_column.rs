use crate::sem::AnalysisError;
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// A completed ALTER COLUMN binding or validation outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlterColumnProblem {
    /// The target relation does not exist or has the wrong kind.
    MissingRelation,
    /// The named column is absent from the resolved relation.
    MissingColumn,
    /// A primary key requires the column to remain NOT NULL.
    PrimaryKey,
    /// An identity column must remain NOT NULL.
    Identity,
    /// DROP EXPRESSION requires a stored generated column.
    NotStoredGenerated,
    /// A virtual generated column does not support this operation.
    VirtualGenerated,
    /// The selected PostgreSQL version does not support this spelling.
    Version,
    /// A statistics target is below -1.
    StatisticsTooLow(i32),
    /// PostgreSQL limits the requested statistics target to 10000.
    StatisticsReduced(i32),
    /// The option is not a recognized column attribute.
    UnknownOption(String),
    /// An option occurs more than once in the same action.
    DuplicateOption(String),
    /// A SET value is missing, invalid, or outside its domain.
    InvalidOption(String),
    /// RESET cannot contain an option value.
    ResetValue(String),
}

impl AlterColumnProblem {
    pub(crate) const fn missing_relation() -> Self {
        Self::MissingRelation
    }
    pub(crate) const fn missing_column() -> Self {
        Self::MissingColumn
    }
    pub(crate) const fn primary_key() -> Self {
        Self::PrimaryKey
    }
    pub(crate) const fn identity() -> Self {
        Self::Identity
    }
    pub(crate) const fn not_stored_generated() -> Self {
        Self::NotStoredGenerated
    }
    pub(crate) const fn virtual_generated() -> Self {
        Self::VirtualGenerated
    }
    pub(crate) const fn version() -> Self {
        Self::Version
    }
    pub(crate) const fn statistics_too_low(value: i32) -> Self {
        Self::StatisticsTooLow(value)
    }
    pub(crate) const fn statistics_reduced(value: i32) -> Self {
        Self::StatisticsReduced(value)
    }
    pub(crate) fn unknown_option(name: &str) -> Self {
        Self::UnknownOption(name.to_owned())
    }
    pub(crate) fn duplicate_option(name: &str) -> Self {
        Self::DuplicateOption(name.to_owned())
    }
    pub(crate) fn invalid_option(name: &str) -> Self {
        Self::InvalidOption(name.to_owned())
    }
    pub(crate) fn reset_value(name: &str) -> Self {
        Self::ResetValue(name.to_owned())
    }

    /// Whether this outcome prevents catalog application.
    pub const fn is_error(&self) -> bool {
        !matches!(self, Self::StatisticsReduced(_))
    }

    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::MissingRelation => "alter_column_missing_relation",
            Self::MissingColumn => "alter_column_missing_column",
            Self::PrimaryKey => "alter_column_primary_key",
            Self::Identity => "alter_column_identity",
            Self::NotStoredGenerated => "alter_column_not_stored_generated",
            Self::VirtualGenerated => "alter_column_virtual_generated",
            Self::Version => "alter_column_version",
            Self::StatisticsTooLow(_) => "alter_column_statistics_too_low",
            Self::StatisticsReduced(_) => "alter_column_statistics_reduced",
            Self::UnknownOption(_) => "alter_column_unknown_option",
            Self::DuplicateOption(_) => "alter_column_duplicate_option",
            Self::InvalidOption(_) => "alter_column_invalid_option",
            Self::ResetValue(_) => "alter_column_reset_value",
        }
    }

    pub(super) fn render_report(
        &self,
        handler: &GraphicalReportHandler,
        source: String,
        table: &crate::sem::TableName,
        column: &crate::sem::ColumnName,
    ) -> Result<String, AnalysisError> {
        let message = match self {
            Self::MissingRelation => "ALTER COLUMN target is absent or is not a table".to_owned(),
            Self::MissingColumn => "ALTER COLUMN target column does not exist".to_owned(),
            Self::PrimaryKey => "a primary-key column must remain NOT NULL".to_owned(),
            Self::Identity => "an identity column must remain NOT NULL".to_owned(),
            Self::NotStoredGenerated => {
                "DROP EXPRESSION requires a stored generated column".to_owned()
            }
            Self::VirtualGenerated => {
                "this operation is not supported for a virtual generated column".to_owned()
            }
            Self::Version => {
                "this ALTER COLUMN spelling is not supported by the selected PostgreSQL version"
                    .to_owned()
            }
            Self::StatisticsTooLow(value) => format!("statistics target {value} is below -1"),
            Self::StatisticsReduced(value) => {
                format!("statistics target {value} is reduced to 10000")
            }
            Self::UnknownOption(name) => format!("unknown column attribute option {name}"),
            Self::DuplicateOption(name) => {
                format!("column attribute option {name} is specified more than once")
            }
            Self::InvalidOption(name) => {
                format!("invalid value for column attribute option {name}")
            }
            Self::ResetValue(name) => format!("RESET option {name} must not include a value"),
        };
        let severity = if self.is_error() {
            Severity::Error
        } else {
            Severity::Warning
        };
        let message = format!("{table}.{column}: {message}");
        let diagnostic = MietteDiagnostic::new(message)
            .with_code(self.code())
            .with_severity(severity);
        let report = Report::new(diagnostic).with_source_code(NamedSource::new("SQL", source));
        let mut rendered = String::new();
        handler.render_report(&mut rendered, &*report)?;
        Ok(rendered)
    }
}

/// A structured ALTER COLUMN diagnostic with its semantic target identity.
#[derive(Clone, Debug)]
pub struct ColumnAttributeProblem {
    table: crate::sem::TableName,
    column: crate::sem::ColumnName,
    reason: AlterColumnProblem,
}
impl ColumnAttributeProblem {
    pub(crate) fn new(
        table: &crate::sem::TableName,
        column: &crate::sem::ColumnName,
        reason: &AlterColumnProblem,
    ) -> Self {
        Self {
            table: table.clone(),
            column: column.clone(),
            reason: reason.clone(),
        }
    }
    /// Requested or resolved table identity.
    pub const fn table(&self) -> &crate::sem::TableName {
        &self.table
    }
    /// Requested column name.
    pub const fn column(&self) -> &crate::sem::ColumnName {
        &self.column
    }
    /// Completed binding/validation outcome.
    pub const fn reason(&self) -> &AlterColumnProblem {
        &self.reason
    }
    pub(super) const fn code(&self) -> &'static str {
        self.reason.code()
    }
    pub(super) fn render_report(
        &self,
        handler: &GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        self.reason
            .render_report(handler, source, &self.table, &self.column)
    }
}
