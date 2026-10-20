use super::DiagnosticMetadata;

/// Structured violations in SELECT clauses, distinct from retained name bindings.
#[derive(Clone, Debug, Eq, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum QueryProblem {
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
}

impl QueryProblem {
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
    /// Diagnostic code.
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::GroupingArgumentNotGrouped => "query_grouping_argument_not_grouped",
            Self::InvalidGroupingPlacement(_) => "query_invalid_grouping_placement",
            Self::TooManyGroupingArguments => "query_too_many_grouping_arguments",
            Self::InvalidCtePlacement => "query_invalid_cte_placement",
            Self::DuplicateWindowDefinition(_) => "query_duplicate_window_definition",
            Self::ReferenceNotFound(_) => "query_reference_not_found",
            Self::AmbiguousReference(_) => "query_reference_ambiguous",
            Self::PositionOutOfRange(_) => "query_position_out_of_range",
            Self::InvalidCompoundOrder => "query_invalid_compound_order",
        }
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        let message = match self {
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
        };
        DiagnosticMetadata::from(message)
    }
}
