use crate::analysis_problem::{ColumnName, DiagnosticMetadata, DiagnosticSeverity, TableName};

/// A completed ALTER COLUMN binding or validation outcome.
#[derive(Clone, Debug, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum AlterColumnProblem {
    /// The parser-retained column replacement is not SQLite engine syntax.
    DefinitionSyntax,
    /// The requested type change occurs twice for one column.
    DuplicateType,
    /// The existing column cannot be assigned to the requested type.
    TypeConversion,
    /// The existing default cannot be assigned to the requested type.
    DefaultConversion,
    /// USING contains a prohibited expression or unresolved binding.
    UsingExpression,
    /// The requested type modifier is invalid.
    InvalidTypmod(String),
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
    /// The selected `PostgreSQL` version does not support this spelling.
    Version,
    /// A statistics target is below -1.
    StatisticsTooLow(i32),
    /// `PostgreSQL` limits the requested statistics target to 10000.
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
    /// Parser extension rejected by the SQLite engine.
    #[must_use]
    pub const fn definition_syntax() -> Self {
        Self::DefinitionSyntax
    }
    /// Repeated type change for one column.
    #[must_use]
    pub const fn duplicate_type() -> Self {
        Self::DuplicateType
    }
    /// Incompatible column assignment conversion.
    #[must_use]
    pub const fn type_conversion() -> Self {
        Self::TypeConversion
    }
    /// Incompatible default assignment conversion.
    #[must_use]
    pub const fn default_conversion() -> Self {
        Self::DefaultConversion
    }
    /// Prohibited or unresolved USING expression.
    #[must_use]
    pub const fn using_expression() -> Self {
        Self::UsingExpression
    }
    /// Invalid requested modifier.
    #[must_use]
    pub fn invalid_typmod(message: &str) -> Self {
        Self::InvalidTypmod(message.to_owned())
    }
    /// Diagnostic missing relation.
    #[must_use]
    pub const fn missing_relation() -> Self {
        Self::MissingRelation
    }
    /// Diagnostic missing column.
    #[must_use]
    pub const fn missing_column() -> Self {
        Self::MissingColumn
    }
    /// Diagnostic primary key.
    #[must_use]
    pub const fn primary_key() -> Self {
        Self::PrimaryKey
    }
    /// Diagnostic identity.
    #[must_use]
    pub const fn identity() -> Self {
        Self::Identity
    }
    /// Diagnostic not stored generated.
    #[must_use]
    pub const fn not_stored_generated() -> Self {
        Self::NotStoredGenerated
    }
    /// Diagnostic virtual generated.
    #[must_use]
    pub const fn virtual_generated() -> Self {
        Self::VirtualGenerated
    }
    /// Diagnostic version.
    #[must_use]
    pub const fn version() -> Self {
        Self::Version
    }
    /// Diagnostic statistics too low.
    #[must_use]
    pub const fn statistics_too_low(value: i32) -> Self {
        Self::StatisticsTooLow(value)
    }
    /// Diagnostic statistics reduced.
    #[must_use]
    pub const fn statistics_reduced(value: i32) -> Self {
        Self::StatisticsReduced(value)
    }
    /// Diagnostic unknown option.
    #[must_use]
    pub fn unknown_option(name: &str) -> Self {
        Self::UnknownOption(name.to_owned())
    }
    /// Diagnostic duplicate option.
    #[must_use]
    pub fn duplicate_option(name: &str) -> Self {
        Self::DuplicateOption(name.to_owned())
    }
    /// Diagnostic invalid option.
    #[must_use]
    pub fn invalid_option(name: &str) -> Self {
        Self::InvalidOption(name.to_owned())
    }
    /// Diagnostic reset value.
    #[must_use]
    pub fn reset_value(name: &str) -> Self {
        Self::ResetValue(name.to_owned())
    }

    /// Whether this outcome prevents catalog application.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        !matches!(self, Self::StatisticsReduced(_))
    }

    /// Diagnostic severity.
    pub(super) const fn severity(&self) -> DiagnosticSeverity {
        if self.is_error() {
            DiagnosticSeverity::Error
        } else {
            DiagnosticSeverity::Warning
        }
    }

    /// Diagnostic code.
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::DefinitionSyntax => "alter_column_definition_syntax",
            Self::DuplicateType => "alter_column_duplicate_type",
            Self::TypeConversion => "alter_column_type_conversion",
            Self::DefaultConversion => "alter_column_default_conversion",
            Self::UsingExpression => "alter_column_using_expression",
            Self::InvalidTypmod(_) => "alter_column_invalid_typmod",
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

    /// Diagnostic presentation data.
    pub(super) fn render_metadata(
        &self,
        table: &TableName,
        column: &ColumnName,
    ) -> DiagnosticMetadata {
        let message = match self {
            Self::DefinitionSyntax => "SQLite does not support ALTER COLUMN ... TO column-definition".to_owned(),
            Self::DuplicateType => "cannot alter the type of one column twice".to_owned(),
            Self::TypeConversion => "column value cannot be assigned to the requested type".to_owned(),
            Self::DefaultConversion => "default cannot be assigned to the requested type independently of USING".to_owned(),
            Self::UsingExpression => "USING requires a resolved scalar expression without subqueries, parameters, aggregates or window/set-returning functions".to_owned(),
            Self::InvalidTypmod(message) => format!("invalid requested type modifier: {message}"),
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

        let message = format!("{table}.{column}: {message}");
        DiagnosticMetadata::from(message)
    }
}

/// A structured ALTER COLUMN diagnostic with its semantic target identity.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct ColumnAttributeProblem {
    table: TableName,
    column: ColumnName,
    reason: AlterColumnProblem,
}
impl ColumnAttributeProblem {
    /// Diagnostic new.
    #[must_use]
    pub fn new(
        table: impl Into<TableName>,
        column: impl Into<ColumnName>,
        reason: &AlterColumnProblem,
    ) -> Self {
        Self {
            table: table.into(),
            column: column.into(),
            reason: reason.clone(),
        }
    }
    /// Requested or resolved table identity.
    #[must_use]
    pub const fn table(&self) -> &TableName {
        &self.table
    }
    /// Requested column name.
    #[must_use]
    pub const fn column(&self) -> &ColumnName {
        &self.column
    }
    /// Completed binding/validation outcome.
    #[must_use]
    pub const fn reason(&self) -> &AlterColumnProblem {
        &self.reason
    }
    /// Diagnostic code.
    pub(super) const fn code(&self) -> &'static str {
        self.reason.code()
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        self.reason.render_metadata(&self.table, &self.column)
    }
}
