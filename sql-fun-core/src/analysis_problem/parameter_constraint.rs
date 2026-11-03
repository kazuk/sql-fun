use crate::analysis_problem::DiagnosticMetadata;

/// Recoverable inconsistency between SQL-derived parameter requirements.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct ParameterConstraintProblem {
    message: String,
}

impl ParameterConstraintProblem {
    /// Diagnostic new.
    #[must_use]
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }

    /// Requirements or parameter uses that could not be reconciled.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.message())
    }
}
