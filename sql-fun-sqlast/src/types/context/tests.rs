use crate::sem::FullName;
use crate::types::{Any, Candidates, CastContext, TypeContext, array::ArrayType};

mod test_helpers {
    use crate::sem::FullName;
    use crate::types::{
        Candidates, CastContext, CastDefinition, TypeContextRead, TypeDefinition, TypeReference,
        base::BaseType,
    };

    pub(super) struct NativeCatalog {
        source: TypeReference,
        target: TypeReference,
        failing: bool,
        postgres: bool,
        scalar_relationship: bool,
        registered: Option<CastDefinition>,
    }

    impl NativeCatalog {
        pub(super) fn available() -> Self {
            Self {
                source: Box::new(BaseType::new(&Self::source_name())),
                target: Box::new(BaseType::new(&Self::target_name())),
                failing: false,
                postgres: false,
                scalar_relationship: true,
                registered: None,
            }
        }

        pub(super) fn with_endpoints(
            source: &dyn TypeDefinition,
            target: &dyn TypeDefinition,
        ) -> Self {
            Self {
                source: source.clone_box(),
                target: target.clone_box(),
                failing: false,
                postgres: true,
                scalar_relationship: true,
                registered: None,
            }
        }

        pub(super) fn postgres() -> Self {
            Self {
                postgres: true,
                ..Self::available()
            }
        }

        pub(super) fn io_only(source: &dyn TypeDefinition, target: &dyn TypeDefinition) -> Self {
            Self {
                scalar_relationship: false,
                ..Self::with_endpoints(source, target)
            }
        }

        pub(super) fn with_registered_cast(
            mut self,
            source: &dyn TypeDefinition,
            target: &dyn TypeDefinition,
            context: &CastContext,
        ) -> Self {
            self.registered = Some(CastDefinition::new(source, target, context));
            self
        }

        pub(super) fn failing() -> Self {
            Self {
                failing: true,
                ..Self::available()
            }
        }

        pub(super) fn source_name() -> FullName {
            FullName::with_schema("native", "source")
        }
        pub(super) fn target_name() -> FullName {
            FullName::with_schema("native", "target")
        }
        pub(super) fn source(&self) -> &dyn TypeDefinition {
            self.source.as_ref()
        }
        pub(super) fn target(&self) -> &dyn TypeDefinition {
            self.target.as_ref()
        }

        fn check(&self) -> Result<(), &'static str> {
            if self.failing {
                Err("native catalog unavailable")
            } else {
                Ok(())
            }
        }

        fn cast(
            &self,
            source: &dyn TypeDefinition,
            target: &dyn TypeDefinition,
            context: &CastContext,
        ) -> Result<Option<CastDefinition>, &'static str> {
            self.check()?;
            if self.scalar_relationship
                && source.same_type(self.source())
                && target.same_type(self.target())
            {
                Ok(Some(CastDefinition::new(source, target, context)))
            } else {
                Ok(None)
            }
        }
    }

    impl TypeContextRead for NativeCatalog {
        type Error = &'static str;

        fn registered_cast(
            &self,
            source: &dyn TypeDefinition,
            target: &dyn TypeDefinition,
        ) -> Result<Option<CastDefinition>, Self::Error> {
            self.check()?;
            if let Some(cast) = &self.registered
                && cast.source().same_type(source)
                && cast.target().same_type(target)
            {
                return Ok(Some(cast.clone()));
            }
            Ok(None)
        }

        fn supports_postgres_coercions(&self) -> bool {
            self.postgres
        }

        fn type_by_exact_name(
            &self,
            name: &FullName,
        ) -> Result<Option<TypeReference>, Self::Error> {
            self.check()?;
            if name == &Self::source_name() {
                Ok(Some(self.source.clone()))
            } else if name == &Self::target_name() {
                Ok(Some(self.target.clone()))
            } else {
                Ok(None)
            }
        }

        fn type_by_name(&self, name: &FullName) -> Result<Option<TypeReference>, Self::Error> {
            self.check()?;
            if name == &FullName::from_local_name("source") {
                Ok(Some(self.source.clone()))
            } else if name == &FullName::from_local_name("target") {
                Ok(Some(self.target.clone()))
            } else {
                self.type_by_exact_name(name)
            }
        }

        fn implicit_cast(
            &self,
            source: &dyn TypeDefinition,
            target: &dyn TypeDefinition,
        ) -> Result<Option<CastDefinition>, Self::Error> {
            self.cast(source, target, &CastContext::implicit())
        }
        fn assignment_cast(
            &self,
            source: &dyn TypeDefinition,
            target: &dyn TypeDefinition,
        ) -> Result<Option<CastDefinition>, Self::Error> {
            self.cast(source, target, &CastContext::assignment())
        }
        fn explicit_cast(
            &self,
            source: &dyn TypeDefinition,
            target: &dyn TypeDefinition,
        ) -> Result<Option<CastDefinition>, Self::Error> {
            self.cast(source, target, &CastContext::explicit())
        }
        fn cast_sources(
            &self,
            target: &dyn TypeDefinition,
            explicit: bool,
        ) -> Result<TypeReference, Self::Error> {
            self.check()?;
            if target.same_type(self.target()) && explicit {
                Ok(Candidates::from_candidates(&[
                    self.source.clone(),
                    self.target.clone(),
                ]))
            } else {
                Ok(Candidates::from_candidates(&[target.clone_box()]))
            }
        }
        fn assignment_sources(
            &self,
            target: &dyn TypeDefinition,
        ) -> Result<TypeReference, Self::Error> {
            if self.scalar_relationship {
                self.cast_sources(target, true)
            } else {
                self.check()?;
                Ok(Candidates::from_candidates(&[target.clone_box()]))
            }
        }
        fn implicit_castable(
            &self,
            source: &dyn TypeDefinition,
        ) -> Result<TypeReference, Self::Error> {
            self.check()?;
            if source.same_type(self.source()) {
                Ok(Candidates::from_candidates(&[
                    self.source.clone(),
                    self.target.clone(),
                ]))
            } else {
                Ok(Candidates::from_candidates(&[source.clone_box()]))
            }
        }
    }
}

#[test]
fn native_provider_owns_name_resolution_without_legacy_catalog() {
    let catalog = test_helpers::NativeCatalog::available();
    let context = TypeContext::new(&catalog);
    let local = FullName::from_local_name("source");
    assert!(context.type_by_exact_name(&local).unwrap().is_none());
    assert!(
        context
            .type_by_name(&local)
            .unwrap()
            .unwrap()
            .same_type(catalog.source())
    );
    assert!(
        context
            .type_by_exact_name(&test_helpers::NativeCatalog::target_name())
            .unwrap()
            .unwrap()
            .same_type(catalog.target())
    );
    assert!(
        context
            .type_by_name(&FullName::from_local_name("absent"))
            .unwrap()
            .is_none()
    );
}

#[rstest::rstest]
#[case::implicit(CastContext::implicit())]
#[case::assignment(CastContext::assignment())]
#[case::explicit(CastContext::explicit())]
fn native_provider_preserves_directed_loaded_casts(#[case] expected: CastContext) {
    let catalog = test_helpers::NativeCatalog::available();
    let context = TypeContext::new(&catalog);
    let result = if expected == CastContext::implicit() {
        context.implicit_cast(catalog.source(), catalog.target())
    } else if expected == CastContext::assignment() {
        context.assignment_cast(catalog.source(), catalog.target())
    } else {
        context.explicit_cast(catalog.source(), catalog.target())
    };
    let cast = result.unwrap().unwrap();
    assert_eq!(cast.context(), expected);
    assert!(cast.source().same_type(catalog.source()));
    assert!(cast.target().same_type(catalog.target()));
    assert!(
        context
            .explicit_cast(catalog.target(), catalog.source())
            .unwrap()
            .is_none()
    );
}

#[test]
fn native_candidate_reads_preserve_inputs_outputs_and_mode() {
    let catalog = test_helpers::NativeCatalog::available();
    let context = TypeContext::new(&catalog);
    let both =
        Candidates::from_candidates(&[catalog.source().clone_box(), catalog.target().clone_box()]);
    let identity = Candidates::from_candidates(&[catalog.target().clone_box()]);
    assert!(
        context
            .cast_sources(catalog.target(), true)
            .unwrap()
            .same_type(both.as_ref())
    );
    assert!(
        context
            .cast_sources(catalog.target(), false)
            .unwrap()
            .same_type(identity.as_ref())
    );
    assert!(
        context
            .assignment_sources(catalog.target())
            .unwrap()
            .same_type(both.as_ref())
    );
    assert!(
        context
            .implicit_castable(catalog.source())
            .unwrap()
            .same_type(both.as_ref())
    );
    assert!(!both.same_type(catalog.target()));
}

#[test]
fn native_error_type_propagates_through_every_read_operation() {
    let catalog = test_helpers::NativeCatalog::failing();
    let context = TypeContext::new(&catalog);
    let expected = "native catalog unavailable";
    assert_eq!(
        context
            .type_by_name(&test_helpers::NativeCatalog::source_name())
            .unwrap_err(),
        expected
    );
    assert_eq!(
        context
            .type_by_exact_name(&test_helpers::NativeCatalog::source_name())
            .unwrap_err(),
        expected
    );
    assert_eq!(
        context
            .implicit_cast(catalog.source(), catalog.target())
            .unwrap_err(),
        expected
    );
    assert_eq!(
        context
            .assignment_cast(catalog.source(), catalog.target())
            .unwrap_err(),
        expected
    );
    assert_eq!(
        context
            .explicit_cast(catalog.source(), catalog.target())
            .unwrap_err(),
        expected
    );
    assert_eq!(
        context.cast_sources(catalog.target(), true).unwrap_err(),
        expected
    );
    assert_eq!(
        context.assignment_sources(catalog.target()).unwrap_err(),
        expected
    );
    assert_eq!(
        context.implicit_castable(catalog.source()).unwrap_err(),
        expected
    );
}

#[test]
fn intrinsic_array_reads_require_no_catalog_backend() {
    let context = TypeContext::new(&());
    let any = Any::new();
    let array = ArrayType::new(&any);
    assert!(context.array_item_type(&array).unwrap().same_type(&any));
    assert!(context.array_item_type(&any).is_none());
}

#[test]
fn native_scalar_provider_supports_unregistered_loaded_arrays() {
    let catalog = test_helpers::NativeCatalog::postgres();
    let context = TypeContext::new(&catalog);
    let source = ArrayType::new(catalog.source());
    let target = ArrayType::new(catalog.target());
    let cast = context.implicit_cast(&source, &target).unwrap().unwrap();
    assert_eq!(cast.context(), CastContext::implicit());
    assert!(cast.source().same_type(&source));
    assert!(cast.target().same_type(&target));
    assert!(context.implicit_cast(&target, &source).unwrap().is_none());
    let nested_source = ArrayType::new(&source);
    let nested_target = ArrayType::new(&target);
    let nested = context
        .explicit_cast(&nested_source, &nested_target)
        .unwrap()
        .unwrap();
    assert_eq!(nested.context(), CastContext::explicit());
    assert!(nested.source().same_type(&nested_source));
    assert!(nested.target().same_type(&nested_target));
}

#[test]
fn domain_array_coercion_preserves_nominal_identity_and_endpoints() {
    use crate::types::{DomainType, TypeDefinition};
    let catalog = test_helpers::NativeCatalog::postgres();
    let context = TypeContext::new(&catalog);
    let source_domain = DomainType::new(
        &FullName::with_schema("native", "source_domain"),
        catalog.source(),
    );
    let target_domain = DomainType::new(
        &FullName::with_schema("native", "target_domain"),
        catalog.target(),
    );
    assert!(!source_domain.same_type(catalog.source()));
    let source = ArrayType::new(&source_domain);
    let target = ArrayType::new(&target_domain);
    let cast = context.assignment_cast(&source, &target).unwrap().unwrap();
    assert_eq!(cast.context(), CastContext::assignment());
    assert!(cast.source().same_type(&source));
    assert!(cast.target().same_type(&target));
    assert!(!cast.source().same_type(&ArrayType::new(catalog.source())));
    assert!(context.assignment_cast(&target, &source).unwrap().is_none());
}

#[rstest::rstest]
#[case::enumeration(Box::new(crate::types::EnumType::new(&FullName::with_schema("native", "state"), &["one".to_owned()])) as crate::types::TypeReference)]
#[case::composite(Box::new(crate::types::CompositeType::new(&None, &crate::types::Members::new(&[]))) as crate::types::TypeReference)]
fn structural_array_lifting_preserves_scalar_categories(
    #[case] source: crate::types::TypeReference,
) {
    use crate::types::base::BaseType;
    let text = BaseType::new(&FullName::with_schema("pg_catalog", "text"));
    let catalog = test_helpers::NativeCatalog::with_endpoints(source.as_ref(), &text);
    let context = TypeContext::new(&catalog);
    let source_array = ArrayType::new(source.as_ref());
    let text_array = ArrayType::new(&text);
    let cast = context
        .explicit_cast(&source_array, &text_array)
        .unwrap()
        .unwrap();
    assert_eq!(cast.context(), CastContext::explicit());
    assert!(cast.source().same_type(&source_array));
    assert!(cast.target().same_type(&text_array));
    assert!(
        context
            .implicit_cast(&text_array, &source_array)
            .unwrap()
            .is_none()
    );
}

#[rstest::rstest]
#[case::enumeration(Box::new(crate::types::EnumType::new(&FullName::with_schema("native", "state"), &["one".to_owned()])) as crate::types::TypeReference)]
#[case::composite(Box::new(crate::types::CompositeType::new(&None, &crate::types::Members::new(&[]))) as crate::types::TypeReference)]
fn postgres_io_coercions_need_no_provider_scalar_cast(#[case] value: crate::types::TypeReference) {
    use crate::types::base::BaseType;
    let text = BaseType::new(&FullName::with_schema("pg_catalog", "text"));
    let catalog = test_helpers::NativeCatalog::io_only(value.as_ref(), &text);
    let context = TypeContext::new(&catalog);
    let value_array = ArrayType::new(value.as_ref());
    let text_array = ArrayType::new(&text);
    assert_eq!(
        context
            .assignment_cast(&value_array, &text_array)
            .unwrap()
            .unwrap()
            .context(),
        CastContext::assignment()
    );
    assert_eq!(
        context
            .explicit_cast(&text_array, &value_array)
            .unwrap()
            .unwrap()
            .context(),
        CastContext::explicit()
    );
    assert!(
        context
            .implicit_cast(&value_array, &text_array)
            .unwrap()
            .is_none()
    );
    assert!(
        context
            .implicit_cast(&text_array, &value_array)
            .unwrap()
            .is_none()
    );
    assert!(
        context
            .assignment_cast(&text_array, &value_array)
            .unwrap()
            .is_none()
    );
}

#[test]
fn registered_explicit_cast_blocks_automatic_assignment_io() {
    use crate::types::{EnumType, base::BaseType};
    let source = EnumType::new(
        &FullName::with_schema("native", "state"),
        &["one".to_owned()],
    );
    let text = BaseType::new(&FullName::with_schema("pg_catalog", "text"));
    let catalog = test_helpers::NativeCatalog::io_only(&source, &text).with_registered_cast(
        &source,
        &text,
        &CastContext::explicit(),
    );
    let context = TypeContext::new(&catalog);
    assert!(context.implicit_cast(&source, &text).unwrap().is_none());
    assert!(context.assignment_cast(&source, &text).unwrap().is_none());
    let cast = context.explicit_cast(&source, &text).unwrap().unwrap();
    assert_eq!(cast.context(), CastContext::explicit());
    assert!(cast.source().same_type(&source));
    assert!(cast.target().same_type(&text));
}

#[test]
fn registered_explicit_array_cast_precedes_implicit_element_cast() {
    let elements = test_helpers::NativeCatalog::postgres();
    let source = ArrayType::new(elements.source());
    let target = ArrayType::new(elements.target());
    let catalog = elements.with_registered_cast(&source, &target, &CastContext::explicit());
    let context = TypeContext::new(&catalog);
    assert!(
        context
            .implicit_cast(catalog.source(), catalog.target())
            .unwrap()
            .is_some()
    );
    assert!(context.implicit_cast(&source, &target).unwrap().is_none());
    assert!(context.assignment_cast(&source, &target).unwrap().is_none());
    let cast = context.explicit_cast(&source, &target).unwrap().unwrap();
    assert_eq!(cast.context(), CastContext::explicit());
    assert!(cast.source().same_type(&source));
    assert!(cast.target().same_type(&target));
}

#[rstest::rstest]
#[case::domain_to_base("domain_to_base")]
#[case::base_to_domain("base_to_domain")]
#[case::domains_over_arrays("domains_over_arrays")]
fn domain_coercion_uses_underlying_relationship_for_each_context(#[case] scenario: &str) {
    use crate::types::{DomainType, TypeDefinition, TypeReference};
    let catalog = test_helpers::NativeCatalog::postgres();
    let (source, target): (TypeReference, TypeReference) = match scenario {
        "domain_to_base" => (
            Box::new(DomainType::new(
                &FullName::with_schema("native", "source_domain"),
                catalog.source(),
            )),
            catalog.target().clone_box(),
        ),
        "base_to_domain" => (
            catalog.source().clone_box(),
            Box::new(DomainType::new(
                &FullName::with_schema("native", "target_domain"),
                catalog.target(),
            )),
        ),
        "domains_over_arrays" => (
            Box::new(DomainType::new(
                &FullName::with_schema("native", "source_domain"),
                &ArrayType::new(catalog.source()),
            )),
            Box::new(DomainType::new(
                &FullName::with_schema("native", "target_domain"),
                &ArrayType::new(catalog.target()),
            )),
        ),
        _ => unreachable!(),
    };
    let context = TypeContext::new(&catalog);
    let implicit = context
        .implicit_cast(source.as_ref(), target.as_ref())
        .unwrap()
        .unwrap();
    let assignment = context
        .assignment_cast(source.as_ref(), target.as_ref())
        .unwrap()
        .unwrap();
    let explicit = context
        .explicit_cast(source.as_ref(), target.as_ref())
        .unwrap()
        .unwrap();
    assert_eq!(implicit.context(), CastContext::implicit());
    assert_eq!(assignment.context(), CastContext::assignment());
    assert_eq!(explicit.context(), CastContext::explicit());
    assert!(implicit.source().same_type(source.as_ref()));
    assert!(implicit.target().same_type(target.as_ref()));
    assert!(
        context
            .implicit_cast(target.as_ref(), source.as_ref())
            .unwrap()
            .is_none()
    );
    assert!(
        context
            .assignment_cast(target.as_ref(), source.as_ref())
            .unwrap()
            .is_none()
    );
    assert!(
        context
            .explicit_cast(target.as_ref(), source.as_ref())
            .unwrap()
            .is_none()
    );
    if let Some(domain) = source.as_any().downcast_ref::<DomainType>() {
        assert!(!domain.same_type(domain.base_type()));
    }
}

#[rstest::rstest]
fn scalar_element_relationship_does_not_imply_scalar_array_cast(
    #[values(false, true)] array_source: bool,
) {
    let catalog = test_helpers::NativeCatalog::postgres();
    let context = TypeContext::new(&catalog);
    let source_array = ArrayType::new(catalog.source());
    let target_array = ArrayType::new(catalog.target());
    let (source, target) = if array_source {
        (
            &source_array as &dyn crate::types::TypeDefinition,
            catalog.target(),
        )
    } else {
        (
            catalog.source(),
            &target_array as &dyn crate::types::TypeDefinition,
        )
    };
    assert!(context.implicit_cast(source, target).unwrap().is_none());
    assert!(context.assignment_cast(source, target).unwrap().is_none());
    assert!(context.explicit_cast(source, target).unwrap().is_none());
}

#[rstest::rstest]
#[case::any(Box::new(Any::new()) as crate::types::TypeReference)]
#[case::unknown(Box::new(crate::types::base::BaseType::new(&FullName::with_schema("pg_catalog", "unknown"))) as crate::types::TypeReference)]
#[case::polymorphic(Box::new(crate::types::base::BaseType::new(&FullName::with_schema("pg_catalog", "anyelement"))) as crate::types::TypeReference)]
fn postgres_io_does_not_fabricate_coercions_for_inference_values(
    #[case] value: crate::types::TypeReference,
) {
    use crate::types::base::BaseType;
    let text = BaseType::new(&FullName::with_schema("pg_catalog", "text"));
    let catalog = test_helpers::NativeCatalog::io_only(value.as_ref(), &text);
    let context = TypeContext::new(&catalog);
    assert!(
        context
            .assignment_cast(value.as_ref(), &text)
            .unwrap()
            .is_none()
    );
    assert!(
        context
            .explicit_cast(value.as_ref(), &text)
            .unwrap()
            .is_none()
    );
    assert!(
        context
            .explicit_cast(&text, value.as_ref())
            .unwrap()
            .is_none()
    );
    assert!(
        context
            .implicit_cast(value.as_ref(), &text)
            .unwrap()
            .is_none()
    );
}

#[test]
fn assignment_candidates_include_provider_explicit_universe_when_io_permits() {
    use crate::types::{EnumType, TypeDefinition, base::BaseType};
    let value = EnumType::new(
        &FullName::with_schema("native", "state"),
        &["one".to_owned()],
    );
    let text = BaseType::new(&FullName::with_schema("pg_catalog", "text"));
    let catalog = test_helpers::NativeCatalog::io_only(&value, &text);
    let context = TypeContext::new(&catalog);
    let expected = Candidates::from_candidates(&[value.clone_box(), text.clone_box()]);
    assert!(
        context
            .assignment_sources(&text)
            .unwrap()
            .same_type(expected.as_ref())
    );
    let implicit = Candidates::from_candidates(&[text.clone_box()]);
    assert!(
        context
            .cast_sources(&text, false)
            .unwrap()
            .same_type(implicit.as_ref())
    );
}
