use super::DiagnosticMessage;

/// Syntax version requirement and the explicitly selected target.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct EngineVersionProblem {
    feature: String,
    required: u32,
    selected: Option<String>,
}

impl EngineVersionProblem {
    pub(super) fn new(feature: &str, required: u32, selected: Option<&str>) -> Self {
        Self {
            feature: feature.to_owned(),
            required,
            selected: selected.map(str::to_owned),
        }
    }
}

impl DiagnosticMessage for EngineVersionProblem {
    fn diagnostic_message(&self) -> String {
        format!(
            "{} requires PostgreSQL {} or later; selected version: {}",
            self.feature,
            self.required,
            self.selected.as_deref().unwrap_or("unspecified"),
        )
    }
}
