use crate::analysis_problem::{DiagnosticMessage, DiagnosticMetadata};

/// A TABLESAMPLE binding, input, or capability problem.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum TableSampleProblem {
    /// sampling method does not exist.
    MethodNotFound(String),
    /// sampling method metadata is unavailable.
    MetadataUnavailable(String),
    /// sampling method is ambiguous.
    MethodAmbiguous(String),
    /// invalid sampling handler.
    InvalidHandler(String),
    /// TABLESAMPLE requires a table or materialized view.
    Relation(String),
    /// invalid sampling argument count.
    Arity(String),
    /// sampling method does not support REPEATABLE.
    Repeatable(String),
    /// invalid TABLESAMPLE argument type.
    ArgumentType(String),
    /// invalid REPEATABLE seed type.
    SeedType(String),
    /// sampling expression cannot use aggregate, window or set-returning functions.
    Expression(String),
}
impl TableSampleProblem {
    /// Report: sampling method does not exist.
    #[must_use]
    pub fn method_not_found(detail: &str) -> Self {
        Self::MethodNotFound(detail.to_owned())
    }
    /// Report: sampling method metadata is unavailable.
    #[must_use]
    pub fn metadata_unavailable(detail: &str) -> Self {
        Self::MetadataUnavailable(detail.to_owned())
    }
    /// Report: sampling method is ambiguous.
    #[must_use]
    pub fn method_ambiguous(detail: &str) -> Self {
        Self::MethodAmbiguous(detail.to_owned())
    }
    /// Report: invalid sampling handler.
    #[must_use]
    pub fn invalid_handler(detail: &str) -> Self {
        Self::InvalidHandler(detail.to_owned())
    }
    /// Report: TABLESAMPLE requires a table or materialized view.
    #[must_use]
    pub fn relation(detail: &str) -> Self {
        Self::Relation(detail.to_owned())
    }
    /// Report: invalid sampling argument count.
    #[must_use]
    pub fn arity(detail: &str) -> Self {
        Self::Arity(detail.to_owned())
    }
    /// Report: sampling method does not support REPEATABLE.
    #[must_use]
    pub fn repeatable(detail: &str) -> Self {
        Self::Repeatable(detail.to_owned())
    }
    /// Report: invalid TABLESAMPLE argument type.
    #[must_use]
    pub fn argument_type(detail: &str) -> Self {
        Self::ArgumentType(detail.to_owned())
    }
    /// Report: invalid REPEATABLE seed type.
    #[must_use]
    pub fn seed_type(detail: &str) -> Self {
        Self::SeedType(detail.to_owned())
    }
    /// Report: sampling expression cannot use aggregate, window or set-returning functions.
    #[must_use]
    pub fn expression(detail: &str) -> Self {
        Self::Expression(detail.to_owned())
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::MethodNotFound(_) => "tablesample_method_not_found",
            Self::MetadataUnavailable(_) => "tablesample_metadata_unavailable",
            Self::MethodAmbiguous(_) => "tablesample_method_ambiguous",
            Self::InvalidHandler(_) => "tablesample_invalid_handler",
            Self::Relation(_) => "tablesample_relation",
            Self::Arity(_) => "tablesample_arity",
            Self::Repeatable(_) => "tablesample_repeatable",
            Self::ArgumentType(_) => "tablesample_argument_type",
            Self::SeedType(_) => "tablesample_seed_type",
            Self::Expression(_) => "tablesample_expression",
        }
    }
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}
impl DiagnosticMessage for TableSampleProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::MethodNotFound(detail) => format!("sampling method does not exist: {detail}"),
            Self::MetadataUnavailable(detail) => {
                format!("sampling method metadata is unavailable: {detail}")
            }
            Self::MethodAmbiguous(detail) => format!("sampling method is ambiguous: {detail}"),
            Self::InvalidHandler(detail) => format!("invalid sampling handler: {detail}"),
            Self::Relation(detail) => {
                format!("TABLESAMPLE requires a table or materialized view: {detail}")
            }
            Self::Arity(detail) => format!("invalid sampling argument count: {detail}"),
            Self::Repeatable(detail) => {
                format!("sampling method does not support REPEATABLE: {detail}")
            }
            Self::ArgumentType(detail) => format!("invalid TABLESAMPLE argument type: {detail}"),
            Self::SeedType(detail) => format!("invalid REPEATABLE seed type: {detail}"),
            Self::Expression(detail) => format!(
                "sampling expression cannot use aggregate, window or set-returning functions: {detail}"
            ),
        }
    }
}
