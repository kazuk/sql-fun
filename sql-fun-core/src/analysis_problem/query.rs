use crate::analysis_problem::{DiagnosticMetadata, MinMaxProblem};

/// Structured violations in SELECT clauses, distinct from retained name bindings.
#[derive(Clone, Debug, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum QueryProblem {
    /// A binary operation has unequal operand widths.
    SetWidth(SetWidthProblem),
    /// A positional result has no legal common type.
    SetType(SetTypeProblem),
    /// An internal list or dialect-specific row value cannot be consumed as a scalar.
    ScalarValueRequired,
    /// Database-name metadata is required for column reference.
    ColumnDatabaseNameRequired(String),
    /// Cross-database column reference is not supported.
    ColumnCrossDatabase(String),
    /// Column qualification is invalid for the selected dialect.
    ColumnQualificationInvalid(String),
    /// Visible source not found for column reference.
    ColumnSourceNotFound(String),
    /// Ambiguous visible source for column reference.
    ColumnSourceAmbiguous(String),
    /// GROUPING argument does not match a grouping expression of its owner.
    GroupingArgumentNotGrouped,
    /// GROUPING is not allowed in this expression context.
    InvalidGroupingPlacement(String),
    /// GROUPING accepts at most 31 arguments.
    TooManyGroupingArguments,

    /// A WITH body violates data-modifying or SELECT INTO placement rules.
    InvalidCtePlacement,
    /// A WINDOW declaration repeats an existing definition name.
    DuplicateWindowDefinition(String),
    /// A clause name has no applicable definition.
    ReferenceNotFound(String),
    /// A clause name identifies more than one applicable definition.
    AmbiguousReference(String),
    /// An ORDER/GROUP BY ordinal is outside the output list.
    PositionOutOfRange(i64),
    /// A compound ORDER BY expression does not identify an output column.
    InvalidCompoundOrder,
    /// A row expansion target does not have a composite type.
    StarProjectionRequiresCompositeType,
    /// Row expansion appears outside a projection target.
    InvalidStarProjectionPlacement,
}

/// Operand shape retained for a rejected set operation.
#[derive(Clone, Debug, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct SetWidthProblem {
    operation: String,
    left: usize,
    right: usize,
}

/// Common-type failure retained with its positional output identity.
#[derive(Clone, Debug, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct SetTypeProblem {
    operation: String,
    column: usize,
    problem: MinMaxProblem,
}

impl QueryProblem {
    /// Report unequal operand column counts.
    #[must_use]
    pub fn set_width(operation: &str, left: usize, right: usize) -> Self {
        Self::SetWidth(SetWidthProblem {
            operation: operation.to_owned(),
            left,
            right,
        })
    }
    /// Report a column's common-type failure without discarding its cause.
    #[must_use]
    pub fn set_type(operation: &str, column: usize, problem: &MinMaxProblem) -> Self {
        Self::SetType(SetTypeProblem {
            operation: operation.to_owned(),
            column,
            problem: problem.clone(),
        })
    }

    /// Reports a non-scalar expression operand.
    #[must_use]
    pub const fn scalar_value_required() -> Self {
        Self::ScalarValueRequired
    }

    /// Database-name metadata is required for column reference.
    #[must_use]
    pub fn column_database_name_required(reference: &str) -> Self {
        Self::ColumnDatabaseNameRequired(reference.to_owned())
    }
    /// Cross-database column reference is not supported.
    #[must_use]
    pub fn column_cross_database(reference: &str) -> Self {
        Self::ColumnCrossDatabase(reference.to_owned())
    }
    /// Column qualification is invalid for the selected dialect.
    #[must_use]
    pub fn column_qualification_invalid(reference: &str) -> Self {
        Self::ColumnQualificationInvalid(reference.to_owned())
    }
    /// Visible source not found for column reference.
    #[must_use]
    pub fn column_source_not_found(reference: &str) -> Self {
        Self::ColumnSourceNotFound(reference.to_owned())
    }
    /// Ambiguous visible source for column reference.
    #[must_use]
    pub fn column_source_ambiguous(reference: &str) -> Self {
        Self::ColumnSourceAmbiguous(reference.to_owned())
    }
    /// Diagnostic grouping argument not grouped.
    #[must_use]
    pub const fn grouping_argument_not_grouped() -> Self {
        Self::GroupingArgumentNotGrouped
    }
    /// Diagnostic invalid grouping placement.
    #[must_use]
    pub fn invalid_grouping_placement(placement: &str) -> Self {
        Self::InvalidGroupingPlacement(placement.to_owned())
    }
    /// Diagnostic too many grouping arguments.
    #[must_use]
    pub const fn too_many_grouping_arguments() -> Self {
        Self::TooManyGroupingArguments
    }

    /// Diagnostic invalid cte placement.
    #[must_use]
    pub const fn invalid_cte_placement() -> Self {
        Self::InvalidCtePlacement
    }
    /// Diagnostic duplicate window definition.
    #[must_use]
    pub fn duplicate_window_definition(name: &str) -> Self {
        Self::DuplicateWindowDefinition(name.to_owned())
    }
    /// Diagnostic reference not found.
    #[must_use]
    pub fn reference_not_found(name: &str) -> Self {
        Self::ReferenceNotFound(name.to_owned())
    }
    /// Diagnostic ambiguous reference.
    #[must_use]
    pub fn ambiguous_reference(name: &str) -> Self {
        Self::AmbiguousReference(name.to_owned())
    }
    /// Diagnostic position out of range.
    #[must_use]
    pub const fn position_out_of_range(position: i64) -> Self {
        Self::PositionOutOfRange(position)
    }
    /// Diagnostic invalid compound order.
    #[must_use]
    pub const fn invalid_compound_order() -> Self {
        Self::InvalidCompoundOrder
    }
    /// Diagnostic row expansion of a scalar value.
    #[must_use]
    pub const fn star_projection_requires_composite_type() -> Self {
        Self::StarProjectionRequiresCompositeType
    }
    /// Diagnostic row expansion outside a target list.
    #[must_use]
    pub const fn invalid_star_projection_placement() -> Self {
        Self::InvalidStarProjectionPlacement
    }
    /// Diagnostic code.
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::SetWidth(_) => "query_set_column_count",
            Self::SetType(_) => "query_set_column_type",
            Self::ScalarValueRequired => "query_scalar_value_required",
            Self::ColumnDatabaseNameRequired(_) => "column_database_name_required",
            Self::ColumnCrossDatabase(_) => "column_cross_database",
            Self::ColumnQualificationInvalid(_) => "column_qualification_invalid",
            Self::ColumnSourceNotFound(_) => "column_source_not_found",
            Self::ColumnSourceAmbiguous(_) => "column_source_ambiguous",
            Self::GroupingArgumentNotGrouped => "query_grouping_argument_not_grouped",
            Self::InvalidGroupingPlacement(_) => "query_invalid_grouping_placement",
            Self::TooManyGroupingArguments => "query_too_many_grouping_arguments",
            Self::InvalidCtePlacement => "query_invalid_cte_placement",
            Self::DuplicateWindowDefinition(_) => "query_duplicate_window_definition",
            Self::ReferenceNotFound(_) => "query_reference_not_found",
            Self::AmbiguousReference(_) => "query_reference_ambiguous",
            Self::PositionOutOfRange(_) => "query_position_out_of_range",
            Self::InvalidCompoundOrder => "query_invalid_compound_order",
            Self::StarProjectionRequiresCompositeType => {
                "query_star_projection_requires_composite_type"
            }
            Self::InvalidStarProjectionPlacement => "query_invalid_star_projection_placement",
        }
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        let message = match self {
            Self::SetWidth(problem) => format!("{} requires equal column counts (left {}, right {})", problem.operation, problem.left, problem.right),
            Self::SetType(problem) => format!("{} column {}: {}", problem.operation, problem.column, problem.problem.message_for(&problem.operation)),
            Self::ScalarValueRequired => "expression requires a scalar value".to_owned(),
            Self::ColumnDatabaseNameRequired(reference) => format!("database-name metadata is required for column reference: {reference}"),
            Self::ColumnCrossDatabase(reference) => format!("cross-database column reference is not supported: {reference}"),
            Self::ColumnQualificationInvalid(reference) => format!("column qualification is invalid for the selected dialect: {reference}"),
            Self::ColumnSourceNotFound(reference) => format!("visible source not found for column reference: {reference}"),
            Self::ColumnSourceAmbiguous(reference) => format!("ambiguous visible source for column reference: {reference}"),

            Self::GroupingArgumentNotGrouped => "arguments to GROUPING must be grouping expressions of the associated query level".to_owned(),
            Self::InvalidGroupingPlacement(placement) => format!("grouping operations are not allowed in {placement}"),
            Self::TooManyGroupingArguments => "GROUPING must have fewer than 32 arguments".to_owned(),
            Self::InvalidCtePlacement => "data-modifying WITH must be at the top level; nested WITH bodies cannot contain modifying WITH or SELECT INTO".to_owned(),
            Self::DuplicateWindowDefinition(name) => format!("duplicate window definition: {name}"),
            Self::ReferenceNotFound(name) => format!("clause reference not found: {name}"),
            Self::AmbiguousReference(name) => format!("ambiguous clause reference: {name}"),
            Self::PositionOutOfRange(position) => {
                format!("ORDER/GROUP BY position {position} is outside the select list")
            }
            Self::InvalidCompoundOrder => {
                "compound ORDER BY must refer to a result column".to_owned()
            }
            Self::StarProjectionRequiresCompositeType => {
                "row expansion requires a composite value".to_owned()
            }
            Self::InvalidStarProjectionPlacement => {
                "row expansion is only valid in a projection target".to_owned()
            }
        };
        DiagnosticMetadata::from(message)
    }
}
