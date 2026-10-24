use super::{DiagnosticMessage, DiagnosticMetadata, TypeReference};

/// Invalid input to a PostgreSQL truth predicate.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum TruthTestProblem {
    /// The operand cannot be implicitly converted to boolean.
    OperandType(TypeReference),
    /// An unknown string literal is not valid PostgreSQL boolean input.
    InvalidLiteral(String),
}

impl TruthTestProblem {
    /// An independently known operand type is not boolean-convertible.
    #[must_use]
    pub fn operand_type(ty: impl Into<TypeReference>) -> Self {
        Self::OperandType(ty.into())
    }

    /// An unknown literal fails boolean input conversion.
    #[must_use]
    pub fn invalid_literal(value: &str) -> Self {
        Self::InvalidLiteral(value.to_owned())
    }

    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::OperandType(_) => "truth_test_operand_type",
            Self::InvalidLiteral(_) => "truth_test_invalid_literal",
        }
    }

    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}

impl DiagnosticMessage for TruthTestProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::OperandType(ty) => format!("truth-test operand must be type boolean, not {ty}"),
            Self::InvalidLiteral(value) => {
                format!("invalid input syntax for type boolean: {value:?}")
            }
        }
    }
}
