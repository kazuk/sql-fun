use std::collections::HashSet;

use crate::{
    context::CastInfoRead,
    sem::{CastDefinition, TypeReference},
};

use super::TestParseContext;

impl TestParseContext {
    /// setup [CastInfoRead::get_implicit_cast] result
    pub fn set_get_implicit_cast_result(
        &mut self,
        source_type: &TypeReference,
        target_type: &TypeReference,
        result_value: Option<CastDefinition>,
    ) {
        let val = self
            .implicit_cast_results
            .entry(source_type.clone())
            .or_default();
        val.insert(target_type.clone(), result_value);
    }

    /// setup [CastInfoRead::get_implicit_castable] result
    pub fn set_get_implicit_castable_result(
        &mut self,
        source_type: &TypeReference,
        target_types: &HashSet<TypeReference>,
    ) {
        self.implicit_castable_results
            .insert(source_type.clone(), target_types.clone());
    }

    /// add a single implicit castable target
    pub fn add_implicit_castable(
        &mut self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) {
        self.implicit_castable_results
            .entry(source_type.clone())
            .or_default()
            .insert(target_type.clone());
    }
}

impl CastInfoRead for TestParseContext {
    fn cast_sources(&self, target: &TypeReference, _explicit: bool) -> HashSet<TypeReference> {
        assert!(
            !self.forbid_type_enumeration,
            "unbounded cast-source inference is forbidden in this scenario"
        );
        self.declared_implicit_cast_sources(target)
    }

    fn declared_implicit_cast_sources(&self, target: &TypeReference) -> HashSet<TypeReference> {
        let mut sources = HashSet::from([target.clone()]);
        for (source, casts) in &self.implicit_cast_results {
            if casts.get(target).is_some_and(Option::is_some) {
                sources.insert(source.clone());
            }
        }
        sources
    }

    fn get_explicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<CastDefinition> {
        self.implicit_cast_results
            .get(source_type)?
            .get(target_type)
            .cloned()
            .flatten()
    }

    fn get_implicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<CastDefinition> {
        let map = self.implicit_cast_results.get(source_type)?;
        map.get(target_type).cloned().flatten()
    }

    fn get_implicit_castable(
        &self,
        source_type: &TypeReference,
    ) -> std::collections::HashSet<TypeReference> {
        self.implicit_castable_results
            .get(source_type)
            .cloned()
            .unwrap_or_default()
    }
}
