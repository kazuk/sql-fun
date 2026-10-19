//! Public loaded-value operations backed by the caller's selected catalog.

use crate::sem::{FullName, ParseContext, TypeReference as LegacyReference};
use crate::types::{CastDefinition, TypeDefinition, TypeReference, array::ArrayType};

use super::{
    ConversionError, Loader, load_assignment_cast, load_assignment_sources, load_cast_sources,
    load_explicit_cast, load_implicit_cast, load_implicit_castable, loaded_reference,
};

/// Read type and cast information using complete loaded type values.
///
/// This facade borrows the caller's selected `ParseContext`. It adds no catalog
/// storage or analyzer environment, and does not migrate existing consumers.
/// Legacy interaction stays in `types::compat`; callers use loaded values for
/// cast inputs and outputs. Named snapshots must match the selected catalog,
/// and unsupported legacy representations return typed errors.
#[derive(Debug)]
pub struct TypeContext<'a, C> {
    context: &'a C,
}

impl<'a, C: ParseContext> TypeContext<'a, C> {
    /// Use this explicitly selected catalog for subsequent loaded-value reads.
    #[must_use]
    pub const fn new(context: &'a C) -> Self {
        Self { context }
    }

    /// Load a type by its exact catalog key, without expanding the search path.
    ///
    /// `Ok(None)` means the key is absent. An existing but incomplete definition
    /// returns a conversion error, rather than an absent or fabricated type.
    pub fn type_by_exact_name(
        &self,
        name: &FullName,
    ) -> Result<Option<TypeReference>, ConversionError> {
        let Some(definition) = self.context.get_type_impl(name) else {
            return Ok(None);
        };
        Loader::with_context(self.context)
            .definition_named(definition, Some(name))
            .map(Some)
    }

    /// Resolve a type name through the selected catalog's search path and load it.
    ///
    /// Qualified names select their exact catalog key. A missing name is
    /// `Ok(None)`; incomplete definitions retain their typed loading errors.
    pub fn type_by_name(&self, name: &FullName) -> Result<Option<TypeReference>, ConversionError> {
        let Some(resolved) = self.context.resolve_type_name(name) else {
            return Ok(None);
        };
        self.type_by_exact_name(&resolved)
    }

    /// Clone the element of a loaded array; return `None` for other categories.
    ///
    /// Reads the complete structural component without a catalog query, so this
    /// also supports nested arrays and constraint-valued elements. Domains are
    /// not flattened. This avoids legacy provider-specific scalar behavior and
    /// dynamic-reference name access; it does not change the legacy API.
    #[must_use]
    pub fn array_item_type(&self, value: &dyn TypeDefinition) -> Option<TypeReference> {
        value
            .as_any()
            .downcast_ref::<ArrayType>()
            .map(|array| array.element_type().clone_box())
    }

    /// Load a direct implicit cast between these complete type snapshots.
    ///
    /// `Ok(None)` is a missing direct relationship. Snapshot mismatches,
    /// unavailable components, and shapes without a faithful legacy reference
    /// return errors. No constraint solving, ranking, or cast-chain search occurs.
    pub fn implicit_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, ConversionError> {
        let source = self.reference(source)?;
        let target = self.reference(target)?;
        load_implicit_cast(self.context, &source, &target)
    }

    /// Load a direct assignment cast, preserving assignment-specific fallback.
    /// Uses the complete-snapshot and error contract of [`Self::implicit_cast`].
    pub fn assignment_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, ConversionError> {
        let source = self.reference(source)?;
        let target = self.reference(target)?;
        load_assignment_cast(self.context, &source, &target)
    }

    /// Load a direct explicit cast without replacing the catalog's context.
    /// Uses the complete-snapshot and error contract of [`Self::implicit_cast`].
    pub fn explicit_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, ConversionError> {
        let source = self.reference(source)?;
        let target = self.reference(target)?;
        load_explicit_cast(self.context, &source, &target)
    }

    /// Load accepted source candidates for the given target.
    ///
    /// `explicit` selects explicit (`true`) or implicit (`false`) eligibility.
    /// Candidates describe input types, separately from the target/result type.
    /// Every enumerated value must load fully; empty success becomes `Never`.
    pub fn cast_sources(
        &self,
        target: &dyn TypeDefinition,
        explicit: bool,
    ) -> Result<TypeReference, ConversionError> {
        load_cast_sources(self.context, &self.reference(target)?, explicit)
    }

    /// Load accepted assignment-source candidates for this target.
    /// Uses the complete-enumeration contract of [`Self::cast_sources`].
    pub fn assignment_sources(
        &self,
        target: &dyn TypeDefinition,
    ) -> Result<TypeReference, ConversionError> {
        load_assignment_sources(self.context, &self.reference(target)?)
    }

    /// Load target candidates reachable by a direct implicit cast from this source.
    ///
    /// This is the output direction, distinct from [`Self::cast_sources`].
    /// Uses exactly the selected catalog's enumeration without transitive search.
    /// Every candidate must load completely; an empty success becomes `Never`.
    pub fn implicit_castable(
        &self,
        source: &dyn TypeDefinition,
    ) -> Result<TypeReference, ConversionError> {
        load_implicit_castable(self.context, &self.reference(source)?)
    }

    fn reference(&self, value: &dyn TypeDefinition) -> Result<LegacyReference, ConversionError> {
        loaded_reference::reference(self.context, value)
    }
}
