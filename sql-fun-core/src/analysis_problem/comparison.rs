use super::{DiagnosticMessage, DiagnosticMetadata};

/// Invalid special or quantified SQL comparison.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum ComparisonProblem {
    /// Outer operands and subquery outputs have different widths.
    Columns(ComparisonColumns),
    /// A comparison operator must return boolean.
    BooleanResult,
}
/// Expected and actual subquery comparison widths.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct ComparisonColumns {
    expected: usize,
    actual: usize,
}
impl ComparisonColumns {
    /// Required number of output columns.
    #[must_use]
    pub const fn expected(&self) -> usize {
        self.expected
    }
    /// Observed number of output columns.
    #[must_use]
    pub const fn actual(&self) -> usize {
        self.actual
    }
}
impl ComparisonProblem {
    /// A completed subquery has an incompatible output width.
    #[must_use]
    pub const fn columns(expected: usize, actual: usize) -> Self {
        Self::Columns(ComparisonColumns { expected, actual })
    }
    /// A selected comparison does not return boolean.
    #[must_use]
    pub const fn boolean_result() -> Self {
        Self::BooleanResult
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::Columns(_) => "comparison_subquery_columns",
            Self::BooleanResult => "comparison_boolean_result",
        }
    }
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}
impl DiagnosticMessage for ComparisonProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::Columns(width) => format!(
                "comparison requires {} subquery columns, found {}",
                width.expected, width.actual
            ),
            Self::BooleanResult => "comparison operator must return boolean".to_owned(),
        }
    }
}
