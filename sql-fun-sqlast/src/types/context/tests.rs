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
    }

    impl NativeCatalog {
        pub(super) fn available() -> Self {
            Self {
                source: Box::new(BaseType::new(&Self::source_name())),
                target: Box::new(BaseType::new(&Self::target_name())),
                failing: false,
            }
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
            if source.same_type(self.source()) && target.same_type(self.target()) {
                Ok(Some(CastDefinition::new(source, target, context)))
            } else {
                Ok(None)
            }
        }
    }

    impl TypeContextRead for NativeCatalog {
        type Error = &'static str;

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
            self.cast_sources(target, true)
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
