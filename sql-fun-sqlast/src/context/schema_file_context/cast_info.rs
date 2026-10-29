use std::{collections::HashSet, str::FromStr};

use crate::{
    context::{CastInfoRead, TypeInfoRead},
    sem::{CastContext, CastDefinition, TypeReference},
};

use crate::context::schema_file_context::SchemaCatalog;

impl CastInfoRead for SchemaCatalog {
    fn get_assignment_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        if let Some(cast) = self.get_implicit_cast(source, target) {
            return Some(cast);
        }
        if let Some(cast) = self.get_explicit_cast(source, target)
            && cast.context().use_in_assignment()
        {
            Some(cast)
        } else {
            self.base.get_assignment_cast(source, target)
        }
    }

    fn declared_implicit_cast_sources(&self, target: &TypeReference) -> HashSet<TypeReference> {
        let mut sources = self.base.declared_implicit_cast_sources(target);
        for (name, _) in self.casts.iter() {
            let source = TypeReference::from_str(&name).expect("canonical concrete type key");
            if self.get_implicit_cast(&source, target).is_some() {
                sources.insert(source);
            }
        }
        sources.insert(target.clone());
        sources
    }

    fn cast_sources(&self, target: &TypeReference, explicit: bool) -> HashSet<TypeReference> {
        let mut sources = self.base.cast_sources(target, explicit);
        for (name, _) in self.casts.iter() {
            let source = TypeReference::from_str(&name).expect("canonical concrete type key");
            sources.insert(source);
        }
        sources.extend(self.types.cast_source_types());
        if explicit {
            // The parent cannot see a target domain declared in this layer.
            let Some(base) = self.domain_base(target) else {
                return HashSet::new();
            };
            sources.extend(self.base.cast_sources(&base, true));
            sources.extend(self.type_references());
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

        let source = self.domain_base(source_type)?;
        let target = self.domain_base(target_type)?;
        if source_type == target_type {
            return self.direct_explicit_cast(source_type, target_type);
        }

        if source_type.is_array() == Some(true) && target_type.is_array() == Some(true) {
            let source_element = source_type.element_type();
            let target_element = target_type.element_type();
            if self.domain_base(&source_element)? != source_element
                || self.domain_base(&target_element)? != target_element
            {
                self.get_explicit_cast(&source_element, &target_element)?;
                return Some(CastDefinition::new(CastContext::explicit()));
            }
        }

        if source != *source_type || target != *target_type {
            // This is a real domain coercion, even when the bases are identical.
            // Do not expose base identity/implicit priority to other contexts.
            self.get_explicit_cast(&source, &target)?;
            return Some(CastDefinition::new(CastContext::explicit()));
        }

        self.direct_explicit_cast(source_type, target_type)
    }

    fn get_implicit_castable(&self, source_type: &TypeReference) -> HashSet<TypeReference> {
        if source_type.is_dynamic() {
            return HashSet::from([source_type.clone()]);
        }
        let Some(source) = self.domain_base(source_type) else {
            return HashSet::from([source_type.clone()]);
        };
        let mut castable = if source != *source_type {
            self.get_implicit_castable(&source)
        } else {
            self.base.get_implicit_castable(source_type)
        };

        if let Some(s) = self.casts.get(&source_type.to_string()) {
            for (k, v) in s.iter() {
                if v.use_in_implicit() {
                    castable.insert(
                        TypeReference::from_str(k.as_str()).expect("canonical concrete type key"),
                    );
                }
            }
        }

        // Enumerate base-derived targets, not unrelated domain declarations.
        // A domain target is checked when a signature or SQL supplies it;
        // catalog growth must not expand bounded parameter inference.
        if source_type.is_array() == Some(true) {
            castable.extend(
                self.get_implicit_castable(&source_type.element_type())
                    .into_iter()
                    .map(|ty| ty.array_type()),
            );
        }
        // source_type -> source_type is always available with no conversion.
        castable.insert(source_type.clone());
        castable.retain(|target| self.get_implicit_cast(source_type, target).is_some());
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

        if source_type == target_type {
            return Some(CastDefinition::new(CastContext::no_conversion()));
        }
        let source = self.domain_base(source_type)?;
        let target = self.domain_base(target_type)?;
        if source != *source_type || target != *target_type {
            let cast = self.get_implicit_cast(&source, &target)?;
            // Domain reduction is coercion, not original type identity; retain
            // an existing base-to-target preferred conversion when present.
            let context = if cast.is_no_conversion() {
                CastContext::implicit()
            } else {
                cast.context()
            };
            return Some(CastDefinition::new(context));
        }

        if source_type.is_array() == Some(true) && target_type.is_array() == Some(true) {
            let source_element = source_type.element_type();
            let target_element = target_type.element_type();
            if self.domain_base(&source_element)? != source_element
                || self.domain_base(&target_element)? != target_element
            {
                self.get_implicit_cast(&source_element, &target_element)?;
                return Some(CastDefinition::new(CastContext::implicit()));
            }
        }

        self.direct_implicit_cast(source_type, target_type)
    }
}

impl SchemaCatalog {
    fn direct_implicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<CastDefinition> {
        if let Some(s) = self.casts.get(&source_type.to_string())
            && let Some(t) = s.get(&target_type.to_string())
        {
            if t.use_in_implicit() {
                Some(t.clone())
            } else {
                None
            }
        } else {
            self.base
                .get_implicit_cast(source_type, target_type)
                .filter(CastDefinition::use_in_implicit)
        }
    }

    /// Preserve local shadowing, inherited lookup, and I/O fallback for base types.
    fn direct_explicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<CastDefinition> {
        if let Some(s) = self.casts.get(&source_type.to_string())
            && let Some(t) = s.get(&target_type.to_string())
        {
            if t.use_in_explicit() {
                Some(t.clone())
            } else {
                None
            }
        } else if let Some(cast) = self.base.get_explicit_cast(source_type, target_type) {
            Some(cast)
        } else {
            self.io_cast(source_type, target_type)
        }
    }
    /// Resolve domains only; an array of domains remains an array type.
    /// Broken or cyclic domain definitions cannot establish cast eligibility.
    fn domain_base(&self, reference: &TypeReference) -> Option<TypeReference> {
        reference.domain_base_type(self)
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
mod test_domain_casts {
    use crate::test_helpers::domain_casts as test_helpers;
    use crate::{
        context::{CastInfoRead, ParseContext},
        sem::PgBuiltInType,
    };

    #[rstest::rstest]
    fn domains_use_explicit_base_conversion_without_implicit_credit(
        #[values(false, true)] frozen: bool,
    ) {
        let mut context = test_helpers::context();
        let domain = test_helpers::domain(&mut context, "amount", Some(&PgBuiltInType::int4()));
        let nested = test_helpers::domain(&mut context, "nested", Some(&domain));
        let context = if frozen {
            crate::test_helpers::schema_layer(std::path::Path::new("child.sql"), &context.freeze())
        } else {
            context
        };
        let cast = context
            .get_explicit_cast(&PgBuiltInType::numeric(), &nested)
            .unwrap();
        assert!(!cast.context().is_no_conversion());
        assert!(!cast.context().use_in_implicit());
        assert!(!cast.context().use_in_assignment());
        assert!(
            context
                .get_explicit_cast(&PgBuiltInType::int4(), &domain)
                .is_some()
        );
        assert!(
            context
                .get_explicit_cast(&nested, &PgBuiltInType::int4())
                .is_some()
        );
        assert!(
            context
                .get_explicit_cast(&PgBuiltInType::text(), &nested)
                .is_some()
        );
        assert!(
            context
                .get_explicit_cast(&PgBuiltInType::bool(), &nested)
                .is_none()
        );
        assert!(
            context
                .get_implicit_cast(&PgBuiltInType::numeric(), &nested)
                .is_none()
        );
        assert!(
            context
                .get_implicit_cast(&PgBuiltInType::int8(), &nested)
                .is_none()
        );
        let sources = context.cast_sources(&nested, true);
        assert!(sources.contains(&PgBuiltInType::numeric()));
        assert!(sources.contains(&PgBuiltInType::int4()));
        assert!(sources.contains(&domain));
        assert!(!sources.contains(&PgBuiltInType::bool()));
    }

    #[rstest::rstest]
    #[case::unresolved(None)]
    #[case::missing(Some("missing"))]
    #[case::cycle(Some("broken"))]
    fn invalid_domain_bases_do_not_establish_a_conversion(#[case] base: Option<&str>) {
        let mut context = test_helpers::context();
        let base = base.map(test_helpers::reference);
        let domain = test_helpers::domain(&mut context, "broken", base.as_ref());
        assert!(
            context
                .get_explicit_cast(&PgBuiltInType::int4(), &domain)
                .is_none()
        );
        assert!(context.cast_sources(&domain, true).is_empty());
        assert!(
            context
                .get_implicit_cast(&domain, &PgBuiltInType::int4())
                .is_none()
        );
        assert_eq!(
            context.get_implicit_castable(&domain),
            std::collections::HashSet::from([domain])
        );
    }

    #[rstest::rstest]
    fn implicit_domain_coercions_preserve_identity_and_enumerated_targets(
        #[values(false, true)] inherited: bool,
        #[values(false, true)] frozen: bool,
    ) {
        let mut context = test_helpers::context();
        let domain = test_helpers::domain(&mut context, "amount", Some(&PgBuiltInType::int4()));
        let nested = test_helpers::domain(&mut context, "nested", Some(&domain));
        let context = if inherited {
            crate::test_helpers::schema_layer(std::path::Path::new("child.sql"), &context.freeze())
        } else {
            context
        };
        let verify = |catalog: &dyn crate::context::CastInfoRead| {
            assert!(
                catalog
                    .get_implicit_cast(&nested, &nested)
                    .unwrap()
                    .is_no_conversion()
            );
            for target in [PgBuiltInType::int4(), PgBuiltInType::int8()] {
                let cast = catalog.get_implicit_cast(&nested, &target).unwrap();
                assert!(cast.context().is_implicit());
                assert!(!cast.is_no_conversion());
            }
            assert!(
                catalog
                    .get_implicit_cast(&nested, &PgBuiltInType::text())
                    .is_none()
            );
            assert!(
                catalog
                    .get_implicit_cast(&nested, &PgBuiltInType::bool())
                    .is_none()
            );
            assert_eq!(
                catalog.get_implicit_castable(&nested),
                std::collections::HashSet::from([
                    nested.clone(),
                    PgBuiltInType::int4(),
                    PgBuiltInType::int8(),
                ])
            );
            assert!(
                catalog
                    .cast_sources(&PgBuiltInType::int4(), false)
                    .contains(&nested)
            );
            assert!(
                !catalog
                    .declared_implicit_cast_sources(&PgBuiltInType::int4())
                    .contains(&nested)
            );
        };
        if frozen {
            verify(&context.freeze());
        } else {
            verify(&context);
        }
    }

    #[rstest::rstest]
    #[case::base(false, false)]
    #[case::array_elements(true, false)]
    #[case::domain_over_array(false, true)]
    fn implicit_domain_targets_follow_base_coercion(
        #[case] array_elements: bool,
        #[case] domain_over_array: bool,
    ) {
        let mut context = test_helpers::context();
        let base = if domain_over_array {
            PgBuiltInType::int4().array_type()
        } else {
            PgBuiltInType::int4()
        };
        let domain = test_helpers::domain(&mut context, "amount", Some(&base));
        let (source, target) = if array_elements {
            (base.array_type(), domain.array_type())
        } else {
            (base, domain)
        };
        let cast = context.get_implicit_cast(&source, &target).unwrap();
        assert!(cast.context().is_implicit());
        assert!(!cast.is_no_conversion());
        assert!(
            context
                .get_assignment_cast(&source, &target)
                .unwrap()
                .context()
                .use_in_assignment()
        );
        assert!(!context.get_implicit_castable(&source).contains(&target));
        assert!(
            context
                .get_implicit_cast(&PgBuiltInType::bool(), &target)
                .is_none()
        );
    }

    #[test]
    fn domain_over_array_reduces_without_erasing_array_element_domains() {
        let mut context = test_helpers::context();
        let array = PgBuiltInType::int4().array_type();
        let domain = test_helpers::domain(&mut context, "amounts", Some(&array));
        let scalar = test_helpers::domain(&mut context, "amount", Some(&PgBuiltInType::int4()));
        assert!(
            context
                .get_implicit_cast(&domain, &array)
                .unwrap()
                .context()
                .is_implicit()
        );
        assert!(
            context
                .get_implicit_cast(&scalar.array_type(), &PgBuiltInType::int4())
                .is_none()
        );
        assert!(
            context
                .get_implicit_cast(&domain, &PgBuiltInType::int4())
                .is_none()
        );
    }
}

#[cfg(test)]
mod test_cast_layers {
    use crate::{
        context::{CastInfoRead, ParseContext},
        sem::CastContext,
    };
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
        use crate::{
            TrieMap,
            context::{ParseContext, SchemaFileContext},
            sem::{CastContext, CastDefinition, FullName, TypeReference},
            test_helpers::TestParseContext,
        };
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
            let mut parent = crate::test_helpers::schema_layer(Path::new("parent.sql"), &root);
            register(&mut parent, &source, &shared, CastContext::implicit());
            register(&mut parent, &source, &inherited, CastContext::implicit());
            let child = crate::test_helpers::schema_layer(Path::new("child.sql"), &parent.freeze());
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
