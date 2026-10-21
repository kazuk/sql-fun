use crate::sem::{AnalysisError, ParseContext, TypeReference};
use std::sync::RwLock;

/// Read the explicit SQL input types installed for this query.
pub trait ParameterTypeHintRead {
    /// Look up a one-based SQL parameter number without translating it at the caller.
    /// Zero, absent hints, and numbers beyond the installed slice return `None`.
    fn get_parameter_typehint(&self, index: usize) -> Option<TypeReference>;
}

/// Install explicit SQL input types before analyzing an independent query.
pub trait ParameterTypeHintWrite {
    /// Resolve zero-based slots (`hints[0]` describes `$1`) using this context's
    /// dialect, catalog, and effective search path. Resolve every hint before
    /// replacing state; an error preserves the previous setup completely.
    /// Empty input and all-`None` input successfully replace the previous setup.
    ///
    /// Hints belong to this query layer, never its shared frozen catalog. Set up
    /// each independent query explicitly, including queries without hints.
    /// Nested queries share the outer query's setup. This operation does not
    /// rewrite executable SQL or install casts.
    ///
    /// # Errors
    /// Returns an error for malformed type syntax or an unresolved catalog type.
    fn setup_typehinting(&self, hints: &[Option<&str>]) -> Result<(), AnalysisError>;
}

/// Mutable request state, deliberately excluded from the frozen schema catalog.
#[derive(Debug, Default)]
pub(crate) struct ParameterTypeHints(RwLock<Vec<Option<TypeReference>>>);

impl ParameterTypeHints {
    pub(crate) fn lookup(&self, index: usize) -> Option<TypeReference> {
        let slot = index.checked_sub(1)?;
        self.0
            .read()
            .expect("parameter hint state poisoned")
            .get(slot)
            .cloned()
            .flatten()
    }

    pub(crate) fn setup(
        &self,
        hints: &[Option<&str>],
        context: &impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let mut resolved = Vec::with_capacity(hints.len());
        for (slot, hint) in hints.iter().enumerate() {
            let ty = if let Some(hint) = hint {
                Some(
                    crate::ast::TypeName::resolve_parameter_hint(hint, context).map_err(
                        |error| {
                            AnalysisError::unexpected_input_error(&format!(
                                "parameter ${} type hint {hint:?}: {error}",
                                slot + 1,
                            ))
                        },
                    )?,
                )
            } else {
                None
            };
            resolved.push(ty);
        }
        *self.0.write().expect("parameter hint state poisoned") = resolved;
        Ok(())
    }
}
