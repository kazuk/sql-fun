//! Legacy catalog backend for the independent loaded-value read contract.

use crate::sem::{FullName, ParseContext};
use crate::types::{CastDefinition, Never, TypeContextRead, TypeDefinition, TypeReference};

use super::{
    ConversionError, Loader, load_assignment_cast, load_assignment_sources, load_cast_sources,
    load_explicit_cast, load_implicit_cast, load_implicit_castable, loaded_reference,
};

impl<C: ParseContext> TypeContextRead for C {
    type Error = ConversionError;

    fn supports_postgres_coercions(&self) -> bool {
        self.dialect() == crate::ast::Dialect::postgres()
    }

    fn existing_array_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, Self::Error> {
        let Some(source) = optional_reference(self, source)? else {
            return Ok(None);
        };
        let Some(target) = optional_reference(self, target)? else {
            return Ok(None);
        };
        load_explicit_cast(self, &source, &target)
    }

    fn type_by_exact_name(&self, name: &FullName) -> Result<Option<TypeReference>, Self::Error> {
        let Some(definition) = self.get_type_impl(name) else {
            return Ok(None);
        };
        Loader::with_context(self)
            .definition_named(definition, Some(name))
            .map(Some)
    }

    fn type_by_name(&self, name: &FullName) -> Result<Option<TypeReference>, Self::Error> {
        let Some(resolved) = self.resolve_type_name(name) else {
            return Ok(None);
        };
        <Self as TypeContextRead>::type_by_exact_name(self, &resolved)
    }

    fn implicit_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, Self::Error> {
        let Some(source) = optional_reference(self, source)? else {
            return Ok(None);
        };
        let Some(target) = optional_reference(self, target)? else {
            return Ok(None);
        };
        load_implicit_cast(self, &source, &target)
    }

    fn assignment_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, Self::Error> {
        let Some(source) = optional_reference(self, source)? else {
            return Ok(None);
        };
        let Some(target) = optional_reference(self, target)? else {
            return Ok(None);
        };
        load_assignment_cast(self, &source, &target)
    }

    fn explicit_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, Self::Error> {
        let Some(source) = optional_reference(self, source)? else {
            return Ok(None);
        };
        let Some(target) = optional_reference(self, target)? else {
            return Ok(None);
        };
        load_explicit_cast(self, &source, &target)
    }

    fn cast_sources(
        &self,
        target: &dyn TypeDefinition,
        explicit: bool,
    ) -> Result<TypeReference, Self::Error> {
        let Some(target) = optional_reference(self, target)? else {
            return Ok(Box::new(Never::new()));
        };
        load_cast_sources(self, &target, explicit)
    }

    fn assignment_sources(
        &self,
        target: &dyn TypeDefinition,
    ) -> Result<TypeReference, Self::Error> {
        let Some(target) = optional_reference(self, target)? else {
            return Ok(Box::new(Never::new()));
        };
        load_assignment_sources(self, &target)
    }

    fn implicit_castable(&self, source: &dyn TypeDefinition) -> Result<TypeReference, Self::Error> {
        let Some(source) = optional_reference(self, source)? else {
            return Ok(Box::new(Never::new()));
        };
        load_implicit_castable(self, &source)
    }
}

// Only retrieve legacy facts when the value has a legacy catalog reference.
// Structural derivation belongs to the permanent TypeContext implementation.
fn optional_reference<C: ParseContext>(
    context: &C,
    value: &dyn TypeDefinition,
) -> Result<Option<crate::sem::TypeReference>, ConversionError> {
    match loaded_reference::reference(context, value) {
        Ok(reference) => Ok(Some(reference)),
        Err(error) if error.has_no_legacy_reference() => Ok(None),
        Err(error) => Err(error),
    }
}
