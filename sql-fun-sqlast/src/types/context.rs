//! Permanent loaded-type context and its provider contract.

use crate::sem::FullName;
use crate::types::{CastDefinition, TypeDefinition, TypeReference, array::ArrayType};

/// Read catalog information using complete loaded types.
///
/// Native providers implement this contract directly. Temporary legacy adapters
/// implement it in `types::compat`, keeping legacy representations and conversion
/// errors out of this interface. This is a loaded-type service, not a substitute
/// for the shared `ParseContext` environment required by SQL analyzers.
pub trait TypeContextRead {
    /// The selected provider's error type.
    type Error;

    /// Load an exact catalog key without expanding the search path.
    /// Absence is `Ok(None)`; a present but incomplete definition is an error.
    fn type_by_exact_name(&self, name: &FullName) -> Result<Option<TypeReference>, Self::Error>;

    /// Resolve the selected name through the provider's search path and load it.
    /// Absence is `Ok(None)`; a present but incomplete definition is an error.
    fn type_by_name(&self, name: &FullName) -> Result<Option<TypeReference>, Self::Error>;

    /// Read a direct implicit cast between complete loaded snapshots.
    /// A missing relationship is `Ok(None)`; provider failures remain errors.
    /// This does not search transitive paths or solve constraints.
    fn implicit_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, Self::Error>;

    /// Read a direct assignment cast using the provider's assignment rules.
    fn assignment_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, Self::Error>;

    /// Read a direct explicit cast, preserving its returned cast context.
    fn explicit_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, Self::Error>;

    /// Load accepted source candidates for this target.
    /// `explicit` selects explicit (`true`) or implicit (`false`) eligibility.
    /// Load every reported candidate or return an error; empty success is `Never`.
    fn cast_sources(
        &self,
        target: &dyn TypeDefinition,
        explicit: bool,
    ) -> Result<TypeReference, Self::Error>;

    /// Load accepted assignment-source candidates for this target.
    /// Load every reported candidate or return an error; empty success is `Never`.
    fn assignment_sources(&self, target: &dyn TypeDefinition)
    -> Result<TypeReference, Self::Error>;

    /// Load direct implicit target candidates reachable from this source.
    /// This is the output direction, distinct from source-candidate enumeration.
    /// Load every reported candidate or return an error; empty success is `Never`.
    fn implicit_castable(&self, source: &dyn TypeDefinition) -> Result<TypeReference, Self::Error>;
}

/// Read type and cast information using the caller's selected provider.
///
/// Owns no catalog storage. The permanent public operations use loaded values;
/// legacy `ParseContext` adaptation is implemented separately in `types::compat`.
/// Existing analyzers retain their shared `ParseContext` environment.
#[derive(Debug)]
pub struct TypeContext<'a, C> {
    context: &'a C,
}

impl<'a, C> TypeContext<'a, C> {
    /// Borrow this explicitly selected provider for subsequent loaded-type reads.
    #[must_use]
    pub const fn new(context: &'a C) -> Self {
        Self { context }
    }

    /// Clone the complete element of a loaded array, or return `None` otherwise.
    ///
    /// This intrinsic read needs no provider. Nested arrays and constraint-valued
    /// elements are preserved; domains are not flattened.
    #[must_use]
    pub fn array_item_type(&self, value: &dyn TypeDefinition) -> Option<TypeReference> {
        value
            .as_any()
            .downcast_ref::<ArrayType>()
            .map(|array| array.element_type().clone_box())
    }
}

impl<C: TypeContextRead> TypeContext<'_, C> {
    /// Load an exact catalog key without expanding the search path.
    /// Absence is `Ok(None)`; a present but incomplete definition is an error.
    pub fn type_by_exact_name(&self, name: &FullName) -> Result<Option<TypeReference>, C::Error> {
        self.context.type_by_exact_name(name)
    }

    /// Resolve the selected name through the provider's search path and load it.
    /// Absence is `Ok(None)`; a present but incomplete definition is an error.
    pub fn type_by_name(&self, name: &FullName) -> Result<Option<TypeReference>, C::Error> {
        self.context.type_by_name(name)
    }

    /// Read a direct implicit cast between complete loaded snapshots.
    /// A missing relationship is `Ok(None)`; provider failures remain errors.
    /// This does not search transitive paths or solve constraints.
    pub fn implicit_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, C::Error> {
        self.context.implicit_cast(source, target)
    }

    /// Read a direct assignment cast using the provider's assignment rules.
    pub fn assignment_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, C::Error> {
        self.context.assignment_cast(source, target)
    }

    /// Read a direct explicit cast, preserving its returned cast context.
    pub fn explicit_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, C::Error> {
        self.context.explicit_cast(source, target)
    }

    /// Load accepted source candidates for this target.
    /// `explicit` selects explicit (`true`) or implicit (`false`) eligibility.
    /// Load every reported candidate or return an error; empty success is `Never`.
    pub fn cast_sources(
        &self,
        target: &dyn TypeDefinition,
        explicit: bool,
    ) -> Result<TypeReference, C::Error> {
        self.context.cast_sources(target, explicit)
    }

    /// Load accepted assignment-source candidates for this target.
    /// Load every reported candidate or return an error; empty success is `Never`.
    pub fn assignment_sources(
        &self,
        target: &dyn TypeDefinition,
    ) -> Result<TypeReference, C::Error> {
        self.context.assignment_sources(target)
    }

    /// Load direct implicit target candidates reachable from this source.
    /// This is the output direction, distinct from source-candidate enumeration.
    /// Load every reported candidate or return an error; empty success is `Never`.
    pub fn implicit_castable(
        &self,
        source: &dyn TypeDefinition,
    ) -> Result<TypeReference, C::Error> {
        self.context.implicit_castable(source)
    }
}

#[cfg(test)]
mod tests;
