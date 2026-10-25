use super::{DiagnosticMetadata, MinMaxProblem, TypeReference};

/// A `PostgreSQL` ARRAY constructor type-resolution failure.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum ArrayProblem {
    /// An empty constructor has no explicit element type.
    Empty,
    /// ARRAY(query) requires exactly one output column.
    SubqueryColumns(usize),
    /// Operand types cannot be matched or coerced to the target.
    Incompatible(Box<TypeReference>, Box<TypeReference>),
    /// Required type-category metadata is unavailable.
    MissingTypeFacts(TypeReference),
}

impl ArrayProblem {
    /// An empty constructor needs an explicit array cast.
    #[must_use]
    pub const fn empty() -> Self {
        Self::Empty
    }
    /// An ARRAY query has an invalid output column count.
    #[must_use]
    pub const fn subquery_columns(count: usize) -> Self {
        Self::SubqueryColumns(count)
    }
    /// Incompatible element types or a prohibited element conversion.
    #[must_use]
    pub fn incompatible(left: impl Into<TypeReference>, right: impl Into<TypeReference>) -> Self {
        Self::Incompatible(Box::new(left.into()), Box::new(right.into()))
    }
    /// Required type metadata is absent.
    #[must_use]
    pub fn missing_type_facts(ty: impl Into<TypeReference>) -> Self {
        Self::MissingTypeFacts(ty.into())
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::Empty => "array_empty",
            Self::SubqueryColumns(_) => "array_subquery_columns",
            Self::Incompatible(..) => "array_incompatible_types",
            Self::MissingTypeFacts(_) => "array_missing_type_facts",
        }
    }
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        let message = match self {
            Self::Empty => {
                "cannot determine type of empty ARRAY; supply an explicit array cast".to_owned()
            }
            Self::Incompatible(left, right) => {
                format!("ARRAY cannot match or convert {left} and {right}")
            }
            Self::MissingTypeFacts(ty) => format!("ARRAY requires type-category metadata for {ty}"),
            Self::SubqueryColumns(count) => {
                format!("ARRAY subquery must return exactly one column, found {count}")
            }
        };
        DiagnosticMetadata::from(message)
    }
}

impl TryFrom<MinMaxProblem> for ArrayProblem {
    type Error = MinMaxProblem;

    fn try_from(value: MinMaxProblem) -> Result<Self, Self::Error> {
        value.into_array_problem()
    }
}
