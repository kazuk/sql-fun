use std::collections::HashSet;

use crate::sem::{CastDefinition, TypeReference};

/// get cast definition
pub trait CastInfoRead {
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
