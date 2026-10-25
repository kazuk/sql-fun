use super::{DiagnosticMessage, DiagnosticMetadata, TypeReference};

/// Invalid operands or equality result for a null-safe distinctness predicate.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum DistinctProblem {
    /// Explicit row constructors have different numbers of fields.
    RowArity(RowArity),
    /// The selected equality operator does not return boolean.
    EqualityResult(TypeReference),
}

/// The field counts of an incompatible explicit row pair.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct RowArity {
    left: usize,
    right: usize,
}

impl DistinctProblem {
    /// Unequal explicit row arities.
    #[must_use]
    pub const fn row_arity(left: usize, right: usize) -> Self {
        Self::RowArity(RowArity { left, right })
    }
    /// A selected equality operator has a non-boolean result.
    pub fn equality_result(ty: impl Into<TypeReference>) -> Self {
        Self::EqualityResult(ty.into())
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::RowArity(_) => "distinct_row_arity",
            Self::EqualityResult(_) => "distinct_equality_result",
        }
    }
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}
impl DiagnosticMessage for DistinctProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::RowArity(arity) => format!(
                "unequal number of entries in distinctness row expressions: {} and {}",
                arity.left, arity.right
            ),
            Self::EqualityResult(ty) => {
                format!("distinctness equality operator must return boolean, not {ty}")
            }
        }
    }
}
