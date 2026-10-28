use super::{DiagnosticMessage, DiagnosticMetadata, TypeReference};

/// A failed explicit COLLATE binding or type applicability check.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum CollateProblem {
    /// The independently known operand type does not support collations.
    OperandType(TypeReference),
    /// More than one definition remains after name visibility is applied.
    Ambiguous(String),
}

impl CollateProblem {
    /// Report a non-collatable operand without changing its type.
    #[must_use]
    pub fn operand_type(ty: impl Into<TypeReference>) -> Self {
        Self::OperandType(ty.into())
    }

    /// Report a completed ambiguous collation lookup.
    #[must_use]
    pub fn ambiguous(name: &str) -> Self {
        Self::Ambiguous(name.to_owned())
    }

    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::OperandType(_) => "collate_operand_type",
            Self::Ambiguous(_) => "collation_name_ambiguous",
        }
    }

    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}

impl DiagnosticMessage for CollateProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::OperandType(ty) => format!("collations are not supported by type {ty}"),
            Self::Ambiguous(name) => format!("collation name is ambiguous: {name}"),
        }
    }
}
