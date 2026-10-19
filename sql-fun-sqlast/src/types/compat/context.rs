//! Legacy catalog backend for the independent loaded-value read contract.

use crate::sem::{FullName, ParseContext};
use crate::types::{CastDefinition, TypeContextRead, TypeDefinition, TypeReference};

use super::{
    ConversionError, Loader, load_assignment_cast, load_assignment_sources, load_cast_sources,
    load_explicit_cast, load_implicit_cast, load_implicit_castable, loaded_reference,
};

impl<C: ParseContext> TypeContextRead for C {
    type Error = ConversionError;

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
        let source = loaded_reference::reference(self, source)?;
        let target = loaded_reference::reference(self, target)?;
        load_implicit_cast(self, &source, &target)
    }

    fn assignment_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, Self::Error> {
        let source = loaded_reference::reference(self, source)?;
        let target = loaded_reference::reference(self, target)?;
        load_assignment_cast(self, &source, &target)
    }

    fn explicit_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, Self::Error> {
        let source = loaded_reference::reference(self, source)?;
        let target = loaded_reference::reference(self, target)?;
        load_explicit_cast(self, &source, &target)
    }

    fn cast_sources(
        &self,
        target: &dyn TypeDefinition,
        explicit: bool,
    ) -> Result<TypeReference, Self::Error> {
        load_cast_sources(self, &loaded_reference::reference(self, target)?, explicit)
    }

    fn assignment_sources(
        &self,
        target: &dyn TypeDefinition,
    ) -> Result<TypeReference, Self::Error> {
        load_assignment_sources(self, &loaded_reference::reference(self, target)?)
    }

    fn implicit_castable(&self, source: &dyn TypeDefinition) -> Result<TypeReference, Self::Error> {
        load_implicit_castable(self, &loaded_reference::reference(self, source)?)
    }
}
