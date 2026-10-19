use std::{collections::HashSet, str::FromStr};

use crate::sem::{CastContext, CastDefinition, CastInfoRead, TypeInfoRead, TypeReference};

use super::SchemaCatalog;

impl CastInfoRead for SchemaCatalog {
    fn registered_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        if source.is_dynamic() || target.is_dynamic() {
            return None;
        }
        if let Some(cast) = self.local_cast(source, target) {
            Some(cast)
        } else {
            self.base.registered_cast(source, target)
        }
    }

    fn get_assignment_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        if source.is_dynamic() || target.is_dynamic() {
            return self.base.get_assignment_cast(source, target);
        }
        if crate::ast::Dialect::from(&self.metadata) == crate::ast::Dialect::postgres()
            && source.is_array() == Some(true)
            && target.is_array() == Some(true)
            && let Some(cast) = self.registered_cast(source, target)
        {
            return if cast.context().use_in_assignment() {
                Some(cast)
            } else {
                None
            };
        }
        if let Some((source, target)) = self.array_elements(source, target) {
            return self.get_assignment_cast(&source, &target);
        }
        if let Some(cast) = self.get_explicit_cast(source, target)
            && cast.context().use_in_assignment()
        {
            Some(cast)
        } else {
            self.base.get_assignment_cast(source, target)
        }
    }

    fn cast_sources(&self, target: &TypeReference, explicit: bool) -> HashSet<TypeReference> {
        let mut sources = self.base.cast_sources(target, explicit);
        for (name, _) in self.casts.iter() {
            let source = TypeReference::from_str(&name).expect("canonical concrete type key");
            sources.insert(source);
        }
        sources.extend(self.types.cast_source_types());
        if explicit {
            sources.extend(Self::io_string_types());
        }
        if crate::ast::Dialect::from(&self.metadata) == crate::ast::Dialect::postgres()
            && !target.is_dynamic()
            && target.is_array() == Some(true)
        {
            for source in self.cast_sources(&target.element_type(), explicit) {
                if !source.is_dynamic() && source.is_array() == Some(false) {
                    sources.insert(TypeReference::concrete_type_ref(source.full_name(), true));
                }
            }
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

        if let Some(cast) = self.prioritized_cast(source_type, target_type) {
            if cast.use_in_explicit() {
                Some(cast)
            } else {
                None
            }
        } else {
            if let Some((source, target)) = self.array_elements(source_type, target_type) {
                return self.get_explicit_cast(&source, &target);
            }
            if let Some(cast) = self.base.get_explicit_cast(source_type, target_type) {
                Some(cast)
            } else {
                self.io_cast(source_type, target_type)
            }
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
                    castable.insert(
                        TypeReference::from_str(k.as_str()).expect("canonical concrete type key"),
                    );
                }
            }
        }

        if crate::ast::Dialect::from(&self.metadata) == crate::ast::Dialect::postgres()
            && source_type.is_array() == Some(true)
        {
            for target in self.get_implicit_castable(&source_type.element_type()) {
                if !target.is_dynamic() && target.is_array() == Some(false) {
                    castable.insert(TypeReference::concrete_type_ref(target.full_name(), true));
                }
            }
            castable.retain(|target| self.get_implicit_cast(source_type, target).is_some());
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

        if let Some(cast) = self.prioritized_cast(source_type, target_type) {
            if cast.use_in_implicit() {
                Some(cast)
            } else {
                None
            }
        } else {
            if let Some((source, target)) = self.array_elements(source_type, target_type) {
                return self.get_implicit_cast(&source, &target);
            }
            self.base.get_implicit_cast(source_type, target_type)
        }
    }
}

impl SchemaCatalog {
    fn local_cast(&self, source: &TypeReference, target: &TypeReference) -> Option<CastDefinition> {
        self.casts
            .get(&source.to_string())?
            .get(&target.to_string())
            .cloned()
    }

    fn prioritized_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        if let Some(cast) = self.local_cast(source, target) {
            return Some(cast);
        }
        // Only array lifting needs to inspect inherited registered rows before
        // delegation. Scalar delegation preserves the base's identity handling.
        if crate::ast::Dialect::from(&self.metadata) == crate::ast::Dialect::postgres()
            && source.is_array() == Some(true)
            && target.is_array() == Some(true)
        {
            self.base.registered_cast(source, target)
        } else {
            None
        }
    }

    fn array_elements(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<(TypeReference, TypeReference)> {
        if crate::ast::Dialect::from(&self.metadata) != crate::ast::Dialect::postgres()
            || source.is_dynamic()
            || target.is_dynamic()
            || source.is_array() != Some(true)
            || target.is_array() != Some(true)
        {
            return None;
        }
        Some((source.element_type(), target.element_type()))
    }

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

    #[test]
    fn local_element_shadow_is_preserved_for_arrays() {
        let (mut context, source, target, _) = test_helpers::cast_layer();
        test_helpers::register(&mut context, &source, &target, CastContext::explicit());
        let array_source = crate::sem::TypeReference::concrete_type_ref(source.full_name(), true);
        let array_target = crate::sem::TypeReference::concrete_type_ref(target.full_name(), true);
        assert!(context.get_implicit_cast(&source, &target).is_none());
        assert!(
            context
                .get_implicit_cast(&array_source, &array_target)
                .is_none()
        );
        assert_eq!(
            context
                .get_explicit_cast(&array_source, &array_target)
                .unwrap()
                .context(),
            CastContext::explicit()
        );
        assert_eq!(
            context
                .get_assignment_cast(&array_source, &array_target)
                .unwrap()
                .context(),
            context
                .get_assignment_cast(&source, &target)
                .unwrap()
                .context()
        );
        assert!(
            !context
                .cast_sources(&array_target, false)
                .contains(&array_source)
        );
        assert!(
            !context
                .get_implicit_castable(&array_source)
                .contains(&array_target)
        );
    }

    #[test]
    fn local_whole_array_cast_shadows_element_cast() {
        let (mut context, source, target, _) = test_helpers::cast_layer();
        let array_source = crate::sem::TypeReference::concrete_type_ref(source.full_name(), true);
        let array_target = crate::sem::TypeReference::concrete_type_ref(target.full_name(), true);
        test_helpers::register(
            &mut context,
            &array_source,
            &array_target,
            CastContext::explicit(),
        );
        assert!(context.get_implicit_cast(&source, &target).is_some());
        assert!(
            context
                .get_implicit_cast(&array_source, &array_target)
                .is_none()
        );
        assert!(
            context
                .get_assignment_cast(&array_source, &array_target)
                .is_none()
        );
        assert!(
            !context
                .cast_sources(&array_target, false)
                .contains(&array_source)
        );
        assert!(
            !context
                .get_implicit_castable(&array_source)
                .contains(&array_target)
        );
    }

    #[test]
    fn inherited_whole_array_cast_shadows_local_element_cast() {
        let (mut parent, source, target, _) = test_helpers::cast_layer();
        let array_source = crate::sem::TypeReference::concrete_type_ref(source.full_name(), true);
        let array_target = crate::sem::TypeReference::concrete_type_ref(target.full_name(), true);
        test_helpers::register(
            &mut parent,
            &array_source,
            &array_target,
            CastContext::explicit(),
        );
        let mut child = crate::test_helpers::schema_layer(
            std::path::Path::new("array-child.sql"),
            &parent.freeze(),
        );
        test_helpers::register(&mut child, &source, &target, CastContext::implicit());
        assert!(child.get_implicit_cast(&source, &target).is_some());
        assert!(
            child
                .get_implicit_cast(&array_source, &array_target)
                .is_none()
        );
        assert_eq!(
            child
                .get_explicit_cast(&array_source, &array_target)
                .unwrap()
                .context(),
            CastContext::explicit(),
        );
        assert!(
            !child
                .cast_sources(&array_target, false)
                .contains(&array_source)
        );
        assert!(
            !child
                .get_implicit_castable(&array_source)
                .contains(&array_target)
        );
    }

    #[test]
    fn loaded_casts_preserve_postgres_layer_shadowing_and_assignment_fallback() {
        assert_loaded_cast_layer(crate::ast::Dialect::postgres());
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn loaded_casts_preserve_sqlite_layer_shadowing_and_assignment_fallback() {
        assert_loaded_cast_layer(crate::ast::Dialect::sqlite());
    }

    fn assert_loaded_cast_layer(dialect: crate::ast::Dialect) {
        use crate::sem::BaseParseContext;
        use crate::test_helpers::TestParseContext;
        use crate::types::compat::{
            load_assignment_cast, load_explicit_cast, load_implicit_cast, load_reference,
        };

        let (mut child, source, shared, inherited) = test_helpers::cast_layer_with_dialect(dialect);
        test_helpers::register(&mut child, &source, &shared, CastContext::explicit());
        let environment = TestParseContext::default().with_dialect(dialect);
        let context = crate::sem::SchemaFileContext::new(
            std::path::Path::new("read.sql"),
            &child.freeze(),
            environment.get_args(),
            environment.metadata(),
        );

        assert!(
            load_implicit_cast(&context, &source, &shared)
                .expect("complete endpoints")
                .is_none()
        );
        let local = load_explicit_cast(&context, &source, &shared)
            .expect("complete endpoints")
            .expect("local explicit relationship");
        assert_eq!(local.context(), crate::types::CastContext::explicit());
        assert!(
            local.source().same_type(
                load_reference(&context, &source)
                    .expect("loaded source")
                    .as_ref()
            )
        );
        assert!(
            local.target().same_type(
                load_reference(&context, &shared)
                    .expect("loaded target")
                    .as_ref()
            )
        );
        let inherited_cast = load_implicit_cast(&context, &source, &inherited)
            .expect("complete endpoints")
            .expect("absent local pair inherits parent cast");
        assert_eq!(
            inherited_cast.context(),
            crate::types::CastContext::from(
                context
                    .get_implicit_cast(&source, &inherited)
                    .expect("legacy inherited cast")
                    .context()
            )
        );
        // Assignment falls back to the parent even though the explicit local
        // relationship shadows the parent's implicit relationship.
        let assignment = load_assignment_cast(&context, &source, &shared)
            .expect("complete endpoints")
            .expect("assignment fallback to parent");
        assert_eq!(assignment.context(), crate::types::CastContext::implicit());
        assert_eq!(
            assignment.context(),
            crate::types::CastContext::from(
                context
                    .get_assignment_cast(&source, &shared)
                    .expect("legacy assignment fallback")
                    .context()
            )
        );
    }

    #[rstest::rstest]
    #[case::dot_scalar(
        crate::sem::FullName::with_schema("a.b", "c"),
        crate::sem::FullName::with_schema("a", "b.c"),
        false,
        false
    )]
    #[case::dot_arrays(
        crate::sem::FullName::with_schema("a.b", "c"),
        crate::sem::FullName::with_schema("a", "b.c"),
        true,
        true
    )]
    #[case::quotes(
        crate::sem::FullName::with_schema("a\"b", "c"),
        crate::sem::FullName::with_schema("a", "b\"c"),
        false,
        false
    )]
    #[case::array_suffix(
        crate::sem::FullName::with_schema("a", "item[]"),
        crate::sem::FullName::with_schema("a", "item"),
        false,
        true
    )]
    #[case::array_flag(
        crate::sem::FullName::with_schema("a", "item"),
        crate::sem::FullName::with_schema("a", "item"),
        false,
        true
    )]
    fn preserves_cast_keys_and_enumerated_types(
        #[case] left: crate::sem::FullName,
        #[case] right: crate::sem::FullName,
        #[case] source_array: bool,
        #[case] other_array: bool,
    ) {
        use crate::sem::TypeReference;
        let (mut child, _, _, _) = test_helpers::cast_layer();
        let source = TypeReference::concrete_type_ref(&left, source_array);
        let other = TypeReference::concrete_type_ref(&right, other_array);
        let target = test_helpers::named_type("target");
        test_helpers::register(&mut child, &source, &target, CastContext::implicit());
        test_helpers::register(&mut child, &other, &target, CastContext::explicit());
        test_helpers::register(&mut child, &target, &source, CastContext::implicit());
        test_helpers::register(&mut child, &target, &other, CastContext::explicit());
        let parent = child.freeze();
        let child =
            crate::test_helpers::schema_layer(std::path::Path::new("cast-child.sql"), &parent);
        assert!(child.get_implicit_cast(&source, &target).is_some());
        assert!(child.get_implicit_cast(&other, &target).is_none());
        assert!(child.get_implicit_cast(&target, &source).is_some());
        assert!(child.get_implicit_cast(&target, &other).is_none());
        let sources = child.cast_sources(&target, false);
        assert!(sources.contains(&source));
        assert!(!sources.contains(&other));
        let targets = child.get_implicit_castable(&target);
        assert!(targets.contains(&source));
        assert!(!targets.contains(&other));
        assert!(child.cast_sources(&target, true).contains(&other));
    }

    mod test_helpers {
        use crate::TrieMap;
        use crate::sem::{
            BaseParseContext, CastContext, CastDefinition, FullName, ParseContext,
            SchemaFileContext, TypeReference,
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
            cast_layer_with_dialect(crate::ast::Dialect::postgres())
        }

        pub(super) fn cast_layer_with_dialect(
            dialect: crate::ast::Dialect,
        ) -> (
            SchemaFileContext,
            TypeReference,
            TypeReference,
            TypeReference,
        ) {
            let mut root = TestParseContext::default().with_dialect(dialect);
            let source = named_type("source");
            let shared = named_type("shared");
            let inherited = named_type("inherited");
            for reference in [&source, &shared, &inherited] {
                root.set_get_type_impl_result(
                    reference.full_name(),
                    &crate::sem::TypeDefinition::from(&crate::sem::CreateEnum::new(
                        reference.full_name(),
                        &["value".to_owned()],
                    )),
                );
            }
            let environment = TestParseContext::default().with_dialect(dialect);
            let root = root.freeze();
            let mut parent = SchemaFileContext::new(
                Path::new("parent.sql"),
                &root,
                environment.get_args(),
                environment.metadata(),
            );
            register(&mut parent, &source, &shared, CastContext::implicit());
            register(&mut parent, &source, &inherited, CastContext::implicit());
            let child = SchemaFileContext::new(
                Path::new("child.sql"),
                &parent.freeze(),
                environment.get_args(),
                environment.metadata(),
            );
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
