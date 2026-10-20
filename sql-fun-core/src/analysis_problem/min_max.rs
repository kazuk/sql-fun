use super::{DiagnosticMetadata, TypeReference};

/// A PostgreSQL GREATEST/LEAST type-resolution failure.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum MinMaxProblem {
    /// No operand was supplied.
    Empty,
    /// Two operand types cannot be coerced to the selected common type.
    Incompatible(Box<TypeReference>, Box<TypeReference>),
    /// Required type-category metadata is unavailable.
    MissingTypeFacts(TypeReference),
    /// Comparison metadata is unavailable for the result type.
    MissingComparisonFacts(TypeReference),
    /// The result type has no PostgreSQL comparison implementation.
    NotOrdered(TypeReference),
}
impl MinMaxProblem {
    /// Diagnostic empty.
    #[must_use]
    pub const fn empty() -> Self {
        Self::Empty
    }
    /// Diagnostic incompatible.
    #[must_use]
    pub fn incompatible(left: impl Into<TypeReference>, right: impl Into<TypeReference>) -> Self {
        Self::Incompatible(Box::new(left.into()), Box::new(right.into()))
    }
    /// Diagnostic missing type facts.
    #[must_use]
    pub fn missing_type_facts(ty: impl Into<TypeReference>) -> Self {
        Self::MissingTypeFacts(ty.into())
    }
    /// Diagnostic missing comparison facts.
    #[must_use]
    pub fn missing_comparison_facts(ty: impl Into<TypeReference>) -> Self {
        Self::MissingComparisonFacts(ty.into())
    }
    /// Diagnostic not ordered.
    #[must_use]
    pub fn not_ordered(ty: impl Into<TypeReference>) -> Self {
        Self::NotOrdered(ty.into())
    }
    /// Diagnostic code.
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::Empty => "min_max_empty",
            Self::Incompatible(..) => "min_max_incompatible_types",
            Self::MissingTypeFacts(_) => "min_max_missing_type_facts",
            Self::MissingComparisonFacts(_) => "min_max_missing_comparison_facts",
            Self::NotOrdered(_) => "min_max_not_ordered",
        }
    }

    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        let message = match self {
            Self::Empty => "GREATEST/LEAST requires at least one argument".to_owned(),
            Self::Incompatible(left, right) => {
                format!("GREATEST/LEAST cannot match {left} and {right}")
            }
            Self::MissingTypeFacts(ty) => {
                format!("GREATEST/LEAST requires type-category metadata for {ty}")
            }
            Self::MissingComparisonFacts(ty) => {
                format!("GREATEST/LEAST requires comparison metadata for {ty}")
            }
            Self::NotOrdered(ty) => {
                format!("GREATEST/LEAST cannot identify a comparison function for {ty}")
            }
        };
        DiagnosticMetadata::from(message)
    }
}
