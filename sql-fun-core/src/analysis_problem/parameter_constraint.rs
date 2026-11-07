use crate::analysis_problem::{DiagnosticMetadata, TypeReference};

/// Recoverable inconsistency between SQL-derived parameter requirements.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct ParameterConstraintProblem {
    message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    binding: Option<ParameterBindingConflict>,
}

impl ParameterConstraintProblem {
    /// Diagnostic new.
    #[must_use]
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            binding: None,
        }
    }

    /// A fixed application binding rejected by another use.
    pub(super) fn binding_conflict(
        index: u32,
        selected: &TypeReference,
        required: &[TypeReference],
    ) -> Self {
        let names: Vec<_> = required.iter().map(TypeReference::as_str).collect();
        Self {
            message: format!(
                "parameter ${index} already has application binding {selected}; conflicting use requires one of ({})",
                names.join(", ")
            ),
            binding: Some(ParameterBindingConflict {
                index,
                selected: selected.clone(),
                required: required.to_vec(),
            }),
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

/// An application input whose local binding choice is ambiguous.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct ParameterBindingAmbiguity {
    index: u32,
    candidates: Vec<TypeReference>,
}

impl ParameterBindingAmbiguity {
    /// Retain the numbered parameter and its local binding candidates.
    pub(super) fn new(index: u32, candidates: &[TypeReference]) -> Self {
        Self {
            index,
            candidates: candidates.to_vec(),
        }
    }

    pub(super) fn message(&self) -> String {
        let mut message = format!(
            "application input binding for parameter ${} is unresolved; supply a supported explicit input type hint; an SQL result cast may be insufficient",
            self.index
        );
        if !self.candidates.is_empty() {
            let candidates: Vec<_> = self.candidates.iter().map(TypeReference::as_str).collect();
            message.push_str("; remaining local binding candidates: ");
            message.push_str(&candidates.join(", "));
        }
        message
    }
}

/// Structured evidence for a use that rejects a previously selected binding.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
struct ParameterBindingConflict {
    index: u32,
    selected: TypeReference,
    required: Vec<TypeReference>,
}
