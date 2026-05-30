use std::{collections::HashSet, str::FromStr};

use crate::sem::{BaseParseContext, CastDefinition, CastInfoRead, FullName, TypeReference};

use super::SchemaFileContext;

impl<TBaseContext> CastInfoRead for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn get_explicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<CastDefinition> {
        if source_type.is_dynamic() || target_type.is_dynamic() {
            return None;
        }

        if let Some(s) = self.casts.get(&source_type.to_string())
            && let Some(t) = s.get(&target_type.to_string())
        {
            if t.use_in_explicit() {
                Some(t.clone())
            } else {
                None
            }
        } else {
            self.base.get_explicit_cast(source_type, target_type)
        }
    }

    fn get_implicit_castable(&self, source_type: &TypeReference) -> HashSet<TypeReference> {
        let mut castable = self.base.get_implicit_castable(source_type);

        if let Some(s) = self.casts.get(&source_type.to_string()) {
            for (k, v) in s.iter() {
                if v.use_in_implicit() {
                    castable.insert(TypeReference::from_full_name(
                        FullName::from_str(k.as_str()).expect("key parseable"),
                    ));
                }
            }
        }

        // source_type -> source_type is always available with no conversion
        if !castable.contains(source_type) {
            castable.insert(source_type.clone());
        }

        castable
    }

    fn get_implicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<CastDefinition> {
        if source_type.is_dynamic() || target_type.is_dynamic() {
            return None;
        }

        if let Some(s) = self.casts.get(&source_type.to_string())
            && let Some(t) = s.get(&target_type.to_string())
        {
            if t.use_in_implicit() {
                Some(t.clone())
            } else {
                None
            }
        } else {
            self.base.get_explicit_cast(source_type, target_type)
        }
    }
}
