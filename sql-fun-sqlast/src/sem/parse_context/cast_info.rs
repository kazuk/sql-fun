use std::collections::HashSet;

use crate::sem::{CastDefinition, TypeReference};

/// get cast definition
pub trait CastInfoRead {
    /// Enumerate source types accepted by a cast to `target`, including identity.
    ///
    /// `explicit` selects explicit-cast eligibility; otherwise only implicit
    /// casts are included. This does not compose transitive cast paths.
    fn cast_sources(&self, target: &TypeReference, explicit: bool) -> HashSet<TypeReference>;

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
