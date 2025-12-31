use crate::sem::{CastDefinition, CastInfoRead, TypeReference};

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
}

impl CastInfoRead for TestParseContext {
    fn get_explicit_cast(
        &self,
        _source_type: &TypeReference,
        _target_type: &TypeReference,
    ) -> Option<CastDefinition> {
        todo!()
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
        _source_type: &TypeReference,
    ) -> std::collections::HashSet<TypeReference> {
        todo!()
    }
}
