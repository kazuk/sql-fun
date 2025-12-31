use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::{
    TrieMap,
    sem::{AnalysisError, CastContext, CastDefinition, CastInfoRead, FullName, TypeReference},
};

pub struct PgBuiltinCasts {}

#[expect(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct CastInfo {
    pub func: String,
    pub method: String,
    pub context: String,
}

// alias: source_type -> target_type -> CastInfo
pub type CastMap = HashMap<String, HashMap<String, CastInfo>>;

impl PgBuiltinCasts {
    pub(super) fn load<P: AsRef<Path>>(
        path: P,
    ) -> Result<TrieMap<TrieMap<CastDefinition>>, AnalysisError> {
        let casts: CastMap = super::deserialize_from_path(path)?;

        let mut result = TrieMap::new();
        for (source, m) in casts {
            let mut group = TrieMap::new();
            for (target, ci) in m {
                let cc = match ci.context.as_str() {
                    "i" => CastContext::Implicit,
                    "a" => CastContext::Assignment,
                    "e" => CastContext::Explicit,
                    e => panic!("unexpected cast context {e}"),
                };
                let def = CastDefinition::new(cc);
                group.insert(&target, def);
            }
            result.insert(&source, group);
        }
        Ok(result)
    }
}

impl CastInfoRead for super::BaseContext {
    fn get_explicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<super::CastDefinition> {
        // TODO: normalize / fix search path
        let source_local_name = source_type.full_name().local_name().to_string();
        let target_local_name = target_type.full_name().local_name().to_string();
        if source_local_name == target_local_name {
            return Some(CastDefinition::new(CastContext::NoConversion));
        }

        let from_source = self.casts.get(&source_local_name)?;
        let cast_def = from_source.get(&target_local_name)?;
        if cast_def.use_in_explicit() {
            Some(cast_def.clone())
        } else {
            None
        }
    }

    fn get_implicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<super::CastDefinition> {
        // TODO: normalize / fix search path
        let source_local_name = source_type.full_name().local_name().to_string();
        let target_local_name = target_type.full_name().local_name().to_string();
        if source_local_name == target_local_name {
            return Some(CastDefinition::new(CastContext::NoConversion));
        }

        let from_source = self.casts.get(&source_local_name)?;
        let cast_def = from_source.get(&target_local_name)?;

        if cast_def.use_in_implicit() {
            Some(cast_def.clone())
        } else {
            None
        }
    }

    fn get_implicit_castable(&self, source_type: &TypeReference) -> HashSet<TypeReference> {
        use std::str::FromStr;

        let source_local_name = source_type.full_name().local_name().to_string();
        let Some(from_source) = &self.casts.get(&source_local_name) else {
            return HashSet::default();
        };
        let mut results = HashSet::new();
        results.insert(source_type.clone()); // source_type -> source_type : NoConversion is always ok

        for (k, _) in from_source.iter() {
            results.insert(TypeReference::from_full_name(
                FullName::from_str(k.as_str()).expect("FullName from_str for builtin type"),
            ));
        }
        results
    }
}
