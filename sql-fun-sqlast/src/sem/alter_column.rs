use crate::sem::{ColumnDefinition, ColumnName, CreateTable, problem::AlterColumnProblem};

/// The completed lookup of an ALTER COLUMN target.
#[derive(Clone, Debug)]
pub enum ColumnAttributeBinding {
    /// The column definition captured during analysis.
    Resolved(Box<ColumnDefinition>),
    /// The relation lookup did not find a table.
    MissingRelation,
    /// The relation exists but the column does not.
    MissingColumn,
}
impl ColumnAttributeBinding {
    pub(crate) fn resolved(column: &ColumnDefinition) -> Self {
        Self::Resolved(Box::new(column.clone()))
    }
    pub(crate) const fn missing_relation() -> Self {
        Self::MissingRelation
    }
    pub(crate) const fn missing_column() -> Self {
        Self::MissingColumn
    }
    /// Bound column and its type/constraints, when lookup succeeded.
    pub fn definition(&self) -> Option<&ColumnDefinition> {
        match self {
            Self::Resolved(column) => Some(column),
            _ => None,
        }
    }
}

/// A retained column attribute name and optional SQL value.
#[derive(Clone, Debug)]
pub struct ColumnAttributeOption {
    name: String,
    value: Option<String>,
}
impl ColumnAttributeOption {
    pub(crate) fn new(name: &str, value: Option<&str>) -> Self {
        Self {
            name: name.to_owned(),
            value: value.map(str::to_owned),
        }
    }
    /// Attribute spelling, including an explicitly supplied namespace.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Requested numeric text; absent for RESET or a valueless SET.
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
    fn valid_numeric_value(&self) -> bool {
        let Some(text) = &self.value else {
            return false;
        };
        let Ok(value) = text.trim().parse::<f64>() else {
            return false;
        };
        if !value.is_finite() || value < -1.0 || value.is_subnormal() {
            return false;
        }
        // PostgreSQL reloptions reject strtod underflow, including values which
        // round to zero. An explicitly zero significand remains valid.
        if value == 0.0 {
            let significand = text.split(['e', 'E']).next().unwrap_or(text);
            return !significand
                .bytes()
                .any(|digit| matches!(digit, b'1'..=b'9'));
        }
        true
    }
    pub(crate) fn validate(&self, reset: bool, preceding: &[Self]) -> Vec<AlterColumnProblem> {
        let mut problems = Vec::new();
        if !reset && !matches!(self.name.as_str(), "n_distinct" | "n_distinct_inherited") {
            problems.push(AlterColumnProblem::unknown_option(&self.name));
        }
        if !reset && preceding.iter().any(|option| option.name == self.name) {
            problems.push(AlterColumnProblem::duplicate_option(&self.name));
        }
        if reset {
            if self.value.is_some() {
                problems.push(AlterColumnProblem::reset_value(&self.name));
            }
        } else if !self.valid_numeric_value() {
            problems.push(AlterColumnProblem::invalid_option(&self.name));
        }
        problems
    }
}

/// Requested and effective PostgreSQL statistics targets after normalization.
#[derive(Clone, Debug)]
pub struct ColumnStatisticsTarget {
    requested: Option<i32>,
    effective: Option<i32>,
}
impl ColumnStatisticsTarget {
    fn new(requested: Option<i32>) -> Self {
        Self {
            requested,
            effective: requested
                .map(|value| value.min(10000))
                .filter(|value| *value >= 0),
        }
    }
    /// Requested integer, or None for the DEFAULT spelling.
    pub const fn requested(&self) -> Option<i32> {
        self.requested
    }
    /// Effective target; None restores the database default.
    pub const fn effective(&self) -> Option<i32> {
        self.effective
    }
}

/// An analyzed column-attribute operation, retaining the user's requested value.
#[derive(Clone, Debug)]
pub enum ColumnAttributeAction {
    /// SET or DROP NOT NULL.
    NotNull(bool),
    /// Requested target; None is the explicit DEFAULT spelling.
    Statistics(ColumnStatisticsTarget),
    /// SET per-column attribute options.
    SetOptions(Vec<ColumnAttributeOption>),
    /// RESET per-column attribute options.
    ResetOptions(Vec<ColumnAttributeOption>),
    /// DROP EXPRESSION, retaining IF EXISTS.
    DropExpression(bool),
}
impl ColumnAttributeAction {
    pub(crate) const fn not_null(value: bool) -> Self {
        Self::NotNull(value)
    }
    pub(crate) fn statistics(value: Option<i32>) -> Self {
        Self::Statistics(ColumnStatisticsTarget::new(value))
    }
    pub(crate) fn options(options: &[ColumnAttributeOption], reset: bool) -> Self {
        if reset {
            Self::ResetOptions(options.to_vec())
        } else {
            Self::SetOptions(options.to_vec())
        }
    }
    pub(crate) const fn drop_expression(if_exists: bool) -> Self {
        Self::DropExpression(if_exists)
    }
    /// Requested NOT NULL setting, when this is a nullability change.
    pub const fn not_null_value(&self) -> Option<bool> {
        if let Self::NotNull(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    /// Statistics request, distinguishing DEFAULT from another operation.
    pub const fn statistics_request(&self) -> Option<Option<i32>> {
        if let Self::Statistics(value) = self {
            Some(value.requested())
        } else {
            None
        }
    }
    /// Requested and effective statistics targets, when this changes statistics.
    pub const fn statistics_target(&self) -> Option<&ColumnStatisticsTarget> {
        if let Self::Statistics(value) = self {
            Some(value)
        } else {
            None
        }
    }
    /// Whether this action resets column options rather than setting them.
    pub const fn resets_options(&self) -> bool {
        matches!(self, Self::ResetOptions(_))
    }
    /// IF EXISTS for DROP EXPRESSION, distinguishing other operations.
    pub const fn drop_expression_if_exists(&self) -> Option<bool> {
        if let Self::DropExpression(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    /// Retained options for a SET or RESET operation.
    pub fn attribute_options(&self) -> Option<&[ColumnAttributeOption]> {
        match self {
            Self::SetOptions(options) | Self::ResetOptions(options) => Some(options),
            _ => None,
        }
    }
    pub(crate) fn validate_options(&self) -> Vec<AlterColumnProblem> {
        match self {
            Self::SetOptions(options) => Self::validate_option_list(options, false),
            Self::ResetOptions(options) => Self::validate_option_list(options, true),
            _ => Vec::new(),
        }
    }
    fn validate_option_list(
        options: &[ColumnAttributeOption],
        reset: bool,
    ) -> Vec<AlterColumnProblem> {
        let mut problems = Vec::new();
        for (index, option) in options.iter().enumerate() {
            problems.extend(option.validate(reset, &options[..index]));
        }
        problems
    }
    pub(crate) fn apply(&self, table: &crate::sem::TableName, column: &mut ColumnDefinition) {
        match self {
            Self::NotNull(value) => column.apply_attribute_nullability(table, *value),
            Self::Statistics(value) => column.apply_statistics_target(value.effective()),
            Self::SetOptions(options) => {
                for option in options {
                    column.apply_distinct_option(option.name(), option.value());
                }
            }
            Self::ResetOptions(options) => {
                for option in options {
                    if matches!(option.name(), "n_distinct" | "n_distinct_inherited") {
                        column.apply_distinct_option(option.name(), None);
                    }
                }
            }
            Self::DropExpression(_) => column.drop_generation_expression(),
        }
    }
}

/// Completed analysis of one ALTER COLUMN attribute operation.
#[derive(Clone, Debug)]
pub struct AlterColumnAttributes {
    column: ColumnName,
    binding: ColumnAttributeBinding,
    action: ColumnAttributeAction,
    problems: Vec<AlterColumnProblem>,
}
impl AlterColumnAttributes {
    pub(crate) fn new(
        column: &ColumnName,
        binding: &ColumnAttributeBinding,
        action: &ColumnAttributeAction,
        problems: &[AlterColumnProblem],
    ) -> Self {
        Self {
            column: column.clone(),
            binding: binding.clone(),
            action: action.clone(),
            problems: problems.to_vec(),
        }
    }
    /// Requested column name.
    pub const fn column(&self) -> &ColumnName {
        &self.column
    }
    /// Completed lookup outcome, independently of diagnostic delivery.
    pub const fn binding(&self) -> &ColumnAttributeBinding {
        &self.binding
    }
    /// Retained operation and requested values.
    pub const fn action(&self) -> &ColumnAttributeAction {
        &self.action
    }
    /// Validation outcomes, including warnings that do not prevent application.
    pub fn problems(&self) -> &[AlterColumnProblem] {
        &self.problems
    }
    /// Whether the operation has a resolved column and no semantic errors.
    pub fn can_apply(&self) -> bool {
        self.binding.definition().is_some()
            && !self.problems.iter().any(AlterColumnProblem::is_error)
    }
    pub(crate) fn apply(&self, table: &mut CreateTable) {
        if self.can_apply() {
            table.apply_column_attributes(&self.column, &self.action);
        }
    }
}
