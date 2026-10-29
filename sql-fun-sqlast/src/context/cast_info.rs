use std::collections::HashSet;

use crate::sem::{CastDefinition, TypeReference};

/// get cast definition
pub trait CastInfoRead {
    /// Enumerate source types accepted by a cast to `target`, including identity.
    ///
    /// `explicit` selects explicit-cast eligibility; otherwise only implicit
    /// casts are included. This does not compose transitive cast paths.
    fn cast_sources(&self, target: &TypeReference, explicit: bool) -> HashSet<TypeReference>;

    /// Identity and explicitly declared implicit-cast sources for a known target.
    /// Unlike `cast_sources`, this must not enumerate catalog types to discover
    /// potential nominal/domain sources. Encountered SQL types are checked
    /// separately with `get_implicit_cast`.
    fn declared_implicit_cast_sources(&self, target: &TypeReference) -> HashSet<TypeReference>;

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

    /// List finite implicit targets derived from `source_type`.
    /// Domain sources include their base-derived targets. Domain declarations
    /// are not discovered as speculative targets; callers check SQL-established
    /// target domains with `get_implicit_cast` instead.
    fn get_implicit_castable(&self, source_type: &TypeReference) -> HashSet<TypeReference>;
}
