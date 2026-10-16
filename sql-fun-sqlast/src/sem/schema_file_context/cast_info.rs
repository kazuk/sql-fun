use std::{collections::HashSet, str::FromStr};

use crate::sem::{
    CastContext, CastDefinition, CastInfoRead, FullName, TypeInfoRead, TypeReference,
};

use super::SchemaCatalog;

impl CastInfoRead for SchemaCatalog {
    fn cast_sources(&self, target: &TypeReference, explicit: bool) -> HashSet<TypeReference> {
        let mut sources = self.base.cast_sources(target, explicit);
        for (name, _) in self.casts.iter() {
            if let Ok(name) = FullName::from_str(&name) {
                sources.insert(TypeReference::from_full_name(name));
            }
        }
        sources.extend(self.types.cast_source_types());
        if explicit {
            sources.extend(Self::io_string_types());
        }
        sources.retain(|source| {
            source == target
                || if explicit {
                    self.get_explicit_cast(source, target).is_some()
                } else {
                    self.get_implicit_cast(source, target).is_some()
                }
        });
        sources
    }

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
            self.base
                .get_explicit_cast(source_type, target_type)
                .or_else(|| self.io_cast(source_type, target_type))
        }
    }

    fn get_implicit_castable(&self, source_type: &TypeReference) -> HashSet<TypeReference> {
        if source_type.is_dynamic() {
            return HashSet::from([source_type.clone()]);
        }
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
            self.base.get_implicit_cast(source_type, target_type)
        }
    }
}

impl SchemaCatalog {
    fn io_string_types() -> [TypeReference; 4] {
        ["text", "varchar", "bpchar", "name"].map(TypeReference::from_master_type_name)
    }

    fn io_cast(&self, source: &TypeReference, target: &TypeReference) -> Option<CastDefinition> {
        self.get_type_impl(source.full_name())?;
        self.get_type_impl(target.full_name())?;
        let strings = Self::io_string_types();
        if strings.contains(source) || strings.contains(target) {
            return Some(CastDefinition::new(CastContext::explicit()));
        }
        None
    }
}

#[cfg(test)]
mod test_cast_layers {
    use crate::sem::{CastContext, CastInfoRead, ParseContext};
    use std::collections::HashSet;

    #[rstest::rstest]
    #[case::implicit(CastContext::implicit(), true)]
    #[case::explicit(CastContext::explicit(), false)]
    #[case::assignment(CastContext::assignment(), false)]
    fn local_cast_eligibility_shadows_parent_and_absent_pairs_fall_back(
        #[case] local_context: CastContext,
        #[case] implicit: bool,
    ) {
        let (mut child, source, shared, inherited) = test_helpers::cast_layer();
        test_helpers::register(&mut child, &source, &shared, local_context);
        let frozen = child.freeze();

        assert_eq!(
            frozen.get_implicit_cast(&source, &shared).is_some(),
            implicit
        );
        assert_eq!(
            frozen
                .get_explicit_cast(&source, &shared)
                .expect("eligible explicit local cast")
                .context(),
            local_context
        );
        assert!(frozen.get_implicit_cast(&source, &inherited).is_some());
        assert!(frozen.get_explicit_cast(&source, &inherited).is_some());
    }

    #[test]
    fn implicit_targets_include_parent_local_and_identity_but_not_explicit_only() {
        let (mut child, source, shared, inherited) = test_helpers::cast_layer();
        let local = test_helpers::named_type("local");
        let explicit = test_helpers::named_type("explicit");
        test_helpers::register(&mut child, &source, &local, CastContext::implicit());
        test_helpers::register(&mut child, &source, &explicit, CastContext::explicit());
        let frozen = child.freeze();

        assert_eq!(
            frozen.get_implicit_castable(&source),
            HashSet::from([source.clone(), shared, inherited, local])
        );
        assert!(!frozen.get_implicit_castable(&source).contains(&explicit));
    }

    mod test_helpers {
        use crate::TrieMap;
        use crate::sem::{
            CastContext, CastDefinition, FullName, ParseContext, SchemaFileContext, TypeReference,
        };
        use crate::test_helpers::TestParseContext;
        use std::path::Path;

        pub(super) fn named_type(name: &str) -> TypeReference {
            TypeReference::from_full_name(FullName::with_schema("fixture", name))
        }

        pub(super) fn cast_layer() -> (
            SchemaFileContext,
            TypeReference,
            TypeReference,
            TypeReference,
        ) {
            let root = TestParseContext::default().freeze();
            let source = named_type("source");
            let shared = named_type("shared");
            let inherited = named_type("inherited");
            let mut parent = SchemaFileContext::new(Path::new("parent.sql"), &root);
            register(&mut parent, &source, &shared, CastContext::implicit());
            register(&mut parent, &source, &inherited, CastContext::implicit());
            let child = SchemaFileContext::new(Path::new("child.sql"), &parent.freeze());
            (child, source, shared, inherited)
        }

        pub(super) fn register(
            context: &mut SchemaFileContext,
            source: &TypeReference,
            target: &TypeReference,
            cast_context: CastContext,
        ) {
            let cast = CastDefinition::new(cast_context);
            if let Some(targets) = context.catalog.casts.get_mut(&source.to_string()) {
                targets.insert(&target.to_string(), cast);
            } else {
                let mut targets = TrieMap::new();
                targets.insert(&target.to_string(), cast);
                context.catalog.casts.insert(&source.to_string(), targets);
            }
        }
    }
}
