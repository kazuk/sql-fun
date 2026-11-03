use crate::analysis_problem::{DiagnosticMessage, DiagnosticMetadata, TypeReference};

/// Invalid shape or comparison in an IN/NOT IN predicate.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum MembershipProblem {
    /// The operands have incompatible widths.
    Width(MembershipWidth),
    /// A selected comparison does not return boolean.
    ComparisonResult(TypeReference),
}

/// Expected and actual membership operand widths.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct MembershipWidth {
    expected: usize,
    actual: usize,
}
impl MembershipProblem {
    /// Record unequal operand widths.
    #[must_use]
    pub const fn width(expected: usize, actual: usize) -> Self {
        Self::Width(MembershipWidth { expected, actual })
    }
    /// Record a non-boolean comparison result.
    pub fn comparison_result(ty: impl Into<TypeReference>) -> Self {
        Self::ComparisonResult(ty.into())
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::Width(_) => "membership_width",
            Self::ComparisonResult(_) => "membership_comparison_result",
        }
    }
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}
impl DiagnosticMessage for MembershipProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::Width(width) => format!(
                "membership requires {} columns, found {}",
                width.expected, width.actual
            ),
            Self::ComparisonResult(ty) => {
                format!("membership comparison must return boolean, not {ty}")
            }
        }
    }
}
