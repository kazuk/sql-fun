use std::collections::HashSet;

use crate::sem::{CastDefinition, TypeReference};

/// get cast definition
pub trait CastInfoRead {
    /// Read a catalog-registered relationship without deriving coercion rules.
    ///
    /// Providers with explicit cast entries must expose the selected entry even
    /// when its context excludes the requested conversion. PostgreSQL array
    /// derivation uses this to preserve whole-array cast precedence. Providers
    /// without registered entries may retain the default absence.
    fn registered_cast(
        &self,
        _source: &TypeReference,
        _target: &TypeReference,
    ) -> Option<CastDefinition> {
        None
    }

    /// Enumerate source types accepted by a cast to `target`, including identity.
    ///
    /// `explicit` selects explicit-cast eligibility; otherwise only implicit
    /// casts are included. This does not compose transitive cast paths.
    fn cast_sources(&self, target: &TypeReference, explicit: bool) -> HashSet<TypeReference>;

    /// Resolve a direct assignment conversion, distinct from explicit coercion.
    fn get_assignment_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        self.get_explicit_cast(source, target)
            .filter(|cast| cast.context().use_in_assignment())
    }

    /// Source types eligible for assignment to `target`, including identity.
    fn assignment_sources(&self, target: &TypeReference) -> HashSet<TypeReference> {
        let mut sources: HashSet<_> = self
            .cast_sources(target, true)
            .into_iter()
            .filter(|source| self.get_assignment_cast(source, target).is_some())
            .collect();
        sources.insert(target.clone());
        sources
    }

    /// get explicit cast operation
    fn get_explicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<CastDefinition>;

    /// get implicit cast operation
    fn get_implicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<CastDefinition>;

    /// list implicit castable types from `source_type`
    fn get_implicit_castable(&self, source_type: &TypeReference) -> HashSet<TypeReference>;
}
