mod resolve;

use crate::context::ParseContext;
use crate::sem::{AnalysisError, TypeReference};

/// Read the explicit SQL input types installed for this query.
pub trait ParameterTypeHintRead {
    /// Look up a one-based SQL parameter number without translating it at the caller.
    /// Zero, absent hints, and numbers beyond the installed slice return `None`.
    fn get_parameter_typehint(&self, index: usize) -> Option<TypeReference>;
}

/// Install explicit SQL input types before analyzing an independent query.
pub trait ParameterTypeHintWrite {
    /// Resolve zero-based slots (`hints[0]` describes `$1`) using this context's
    /// catalog and effective search path. [`crate::sem::parse_client_type`] interprets
    /// SQL type spelling using this context's dialect and rejects type modifiers.
    /// Resolve every hint before replacing state; an error preserves the previous setup completely.
    /// Empty input and all-`None` input successfully replace the previous setup.
    /// PostgreSQL `unknown` hints leave the corresponding slot unspecified.
    ///
    /// Hints belong to this query layer, never its shared frozen catalog. Set up
    /// each independent query explicitly, including queries without hints.
    /// Nested queries share the outer query's setup. This operation does not
    /// rewrite executable SQL or install casts.
    ///
    /// # Errors
    /// Returns an error for malformed type syntax or an unresolved catalog type.
    fn setup_typehinting(&mut self, hints: &[Option<&str>]) -> Result<(), AnalysisError>;
}

/// Mutable request state, deliberately excluded from the frozen schema catalog.
#[cfg_attr(test, derive(Clone))]
#[derive(Debug, Default)]
pub(crate) struct ParameterTypeHints(Vec<Option<TypeReference>>);

impl ParameterTypeHints {
    pub(crate) fn lookup(&self, index: usize) -> Option<TypeReference> {
        let slot = index.checked_sub(1)?;
        self.0.get(slot).cloned().flatten()
    }

    pub(crate) fn resolve(
        hints: &[Option<&str>],
        context: &impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut resolved = Vec::with_capacity(hints.len());
        for (slot, hint) in hints.iter().enumerate() {
            let ty = if let Some(hint) = hint {
                self::resolve::resolve_parameter_hint(slot, hint, context)?
            } else {
                None
            };
            resolved.push(ty);
        }
        Ok(Self(resolved))
    }
}

#[cfg(test)]
mod tests;
