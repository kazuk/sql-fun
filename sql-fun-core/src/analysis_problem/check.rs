use crate::analysis_problem::{DiagnosticMessage, DiagnosticMetadata};

/// A CHECK definition that cannot be applied to the analysis catalog.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum CheckProblem {
    /// The expression cannot be coerced to boolean.
    BooleanType,
    /// An unknown string is not valid boolean input.
    BooleanLiteral(String),
    /// CHECK cannot contain a subquery, aggregate, window or set-returning expression.
    ExpressionPlacement,
    /// CHECK cannot contain a bind parameter.
    Parameter,
    /// CHECK constraints cannot be deferred.
    Deferrability,
    /// An explicit constraint name is already used on this table.
    DuplicateName(String),
}
impl CheckProblem {
    pub(super) const CODE: &'static str = "check_constraint";
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}
impl DiagnosticMessage for CheckProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::BooleanType => "CHECK expression must have boolean type".into(),
            Self::BooleanLiteral(value) => format!("invalid CHECK boolean input: {value}"),
            Self::ExpressionPlacement => {
                "CHECK cannot contain subqueries, aggregates, windows or set-returning expressions"
                    .into()
            }
            Self::Parameter => "CHECK cannot contain bind parameters".into(),
            Self::Deferrability => "CHECK constraints cannot be deferred".into(),
            Self::DuplicateName(name) => {
                format!("constraint name already exists on this table: {name}")
            }
        }
    }
}
