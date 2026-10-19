//! Permanent loaded-type context and its provider contract.

use crate::ast::Dialect;
use crate::sem::FullName;
use crate::types::{
    CastContext, CastDefinition, CastQueryError, CastQueryOutcome, TypeDefinition, TypeReference,
    array::ArrayType, cast::CastResolver,
};

/// Read catalog information using complete loaded types.
///
/// Native providers implement this contract directly. Temporary legacy adapters
/// implement it in `types::compat`, keeping legacy representations and conversion
/// errors out of this interface. This is a loaded-type service, not a substitute
/// for the shared `ParseContext` environment required by SQL analyzers.
pub trait TypeContextRead {
    /// The selected provider's error type.
    type Error;

    /// Return the SQL dialect selected by the shared analysis environment.
    /// Legacy adapters forward `ParseContext::dialect` without a separate policy flag.
    fn dialect(&self) -> Dialect;

    /// Return an exact registered relationship, without inferred fallback rules.
    ///
    /// Providers with access to registration provenance can expose it here so
    /// an explicit-only entry blocks automatic implicit or assignment rules.
    /// Providers without that provenance may retain the default absence.
    fn registered_cast(
        &self,
        _source: &dyn TypeDefinition,
        _target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, Self::Error> {
        Ok(None)
    }

    /// Return an existing whole-array relationship before component derivation.
    ///
    /// The context applies the returned eligibility to every requested mode.
    /// This is a provider fact, not necessarily an explicit catalog row: legacy
    /// providers may already derive builtin array relationships.
    fn existing_array_cast(
        &self,
        _source: &dyn TypeDefinition,
        _target: &dyn TypeDefinition,
    ) -> Result<Option<CastDefinition>, Self::Error> {
        Ok(None)
    }

    /// Load whole operator signatures using the selected provider's name policy.
    /// Absence is `Ok(None)`; unavailable signature components are errors.
    fn operator_by_name(
        &self,
        name: &crate::sem::OperatorName,
        form: crate::types::OperatorForm,
    ) -> Result<Option<crate::types::OperatorOverloads>, Self::Error>;

    /// Load complete function signatures at an exact catalog key.
    /// Absence is `Ok(None)`; incomplete producer metadata is an error.
    fn function_by_exact_name(
        &self,
        name: &FullName,
    ) -> Result<Option<crate::types::FunctionOverloads>, Self::Error>;

    /// Resolve the function name through the selected provider's search path.
    fn function_by_name(
        &self,
        name: &FullName,
    ) -> Result<Option<crate::types::FunctionOverloads>, Self::Error>;

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
    /// Load whole operator signatures for the explicitly supplied name and form.
    /// Uses the provider's existing lookup policy, without independently choosing a dialect.
    pub fn operator_by_name(
        &self,
        name: &crate::sem::OperatorName,
        form: crate::types::OperatorForm,
    ) -> Result<Option<crate::types::OperatorOverloads>, C::Error> {
        self.context.operator_by_name(name, form)
    }

    /// Load whole signatures at an exact key, preserving provider order and linkage.
    pub fn function_by_exact_name(
        &self,
        name: &FullName,
    ) -> Result<Option<crate::types::FunctionOverloads>, C::Error> {
        self.context.function_by_exact_name(name)
    }

    /// Load whole signatures using the selected provider's function search path.
    pub fn function_by_name(
        &self,
        name: &FullName,
    ) -> Result<Option<crate::types::FunctionOverloads>, C::Error> {
        self.context.function_by_name(name)
    }

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
    /// A determinate missing relationship is resolved `None`; binding-dependent
    /// queries are deferred. Provider failures remain errors.
    /// This does not search transitive paths or solve constraints.
    pub fn implicit_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<CastQueryOutcome<Option<CastDefinition>>, CastQueryError<C::Error>> {
        CastResolver::new(self.context).query_cast(source, target, CastContext::implicit())
    }

    /// Read a direct assignment cast, or defer if polymorphic binding is needed.
    pub fn assignment_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<CastQueryOutcome<Option<CastDefinition>>, CastQueryError<C::Error>> {
        CastResolver::new(self.context).query_cast(source, target, CastContext::assignment())
    }

    /// Read a direct explicit cast, or defer if polymorphic binding is needed.
    pub fn explicit_cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
    ) -> Result<CastQueryOutcome<Option<CastDefinition>>, CastQueryError<C::Error>> {
        CastResolver::new(self.context).query_cast(source, target, CastContext::explicit())
    }

    /// Load accepted source candidates for this target.
    /// `explicit` selects explicit (`true`) or implicit (`false`) eligibility.
    /// Concrete empty results resolve to `Never`; binding-dependent enumeration
    /// is deferred rather than presented as an empty or partial candidate set.
    pub fn cast_sources(
        &self,
        target: &dyn TypeDefinition,
        explicit: bool,
    ) -> Result<CastQueryOutcome<TypeReference>, CastQueryError<C::Error>> {
        CastResolver::new(self.context).query_sources(
            target,
            if explicit {
                CastContext::explicit()
            } else {
                CastContext::implicit()
            },
        )
    }

    /// Load accepted assignment-source candidates for this target.
    /// Concrete empty results resolve to `Never`; binding-dependent enumeration
    /// is deferred rather than presented as an empty or partial candidate set.
    pub fn assignment_sources(
        &self,
        target: &dyn TypeDefinition,
    ) -> Result<CastQueryOutcome<TypeReference>, CastQueryError<C::Error>> {
        CastResolver::new(self.context).query_sources(target, CastContext::assignment())
    }

    /// Load direct implicit target candidates reachable from this source.
    /// This is the output direction, distinct from source-candidate enumeration.
    /// Concrete empty results resolve to `Never`; binding-dependent enumeration
    /// is deferred rather than presented as an empty or partial candidate set.
    pub fn implicit_castable(
        &self,
        source: &dyn TypeDefinition,
    ) -> Result<CastQueryOutcome<TypeReference>, CastQueryError<C::Error>> {
        CastResolver::new(self.context).query_targets(source)
    }
}

#[cfg(test)]
mod tests;
