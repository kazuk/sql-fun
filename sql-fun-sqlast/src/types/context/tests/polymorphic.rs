use super::test_helpers::NativeCatalog;
use crate::ast::Dialect;
use crate::sem::FullName;
use crate::types::{
    Candidates, CastContext, CastQueryDirection, CastQueryOutcome, CompositeType, DomainType,
    ImplicitCompositeType, Member, Members, MultirangeType, PolymorphicCategory as C,
    PolymorphicPosition as P, PolymorphicRole as R, PolymorphicType, RangeType,
    SignaturePolymorphism, TableRowType, TypeContext, TypeContextRead, TypeDefinition,
    TypeReference, array::ArrayType, base::BaseType,
};

mod test_helpers {
    use super::{
        ArrayType, BaseType, C, Candidates, CompositeType, DomainType, FullName,
        ImplicitCompositeType, Member, Members, MultirangeType, P, R, RangeType,
        SignaturePolymorphism, TableRowType, TypeDefinition, TypeReference,
    };

    pub(super) fn variable(category: C) -> TypeReference {
        SignaturePolymorphism::new(&[P::new(category, R::input())])
            .unwrap()
            .types()[0]
            .clone_box()
    }
    pub(super) fn concrete() -> BaseType {
        BaseType::new(&FullName::with_schema("pg_catalog", "int4"))
    }
    pub(super) fn wrapped(shape: &str) -> TypeReference {
        let variable = variable(C::anyelement());
        let name = FullName::with_schema("native", "wrapped");
        match shape {
            "leaf" => variable,
            "array" => Box::new(ArrayType::new(variable.as_ref())),
            "range" => Box::new(RangeType::new(&name, variable.as_ref())),
            "multirange" => Box::new(MultirangeType::new(
                &name,
                &RangeType::new(&name, variable.as_ref()),
            )),
            "domain" => Box::new(DomainType::new(&name, variable.as_ref())),
            "composite" => Box::new(CompositeType::new(
                &None,
                &Members::new(&[Member::new(&None, variable.as_ref(), None)]),
            )),
            "implicit_record" => {
                Box::new(ImplicitCompositeType::new(&Members::new(&[Member::new(
                    &None,
                    variable.as_ref(),
                    None,
                )])))
            }
            "row" => Box::new(TableRowType::new(
                &name,
                &Members::new(&[Member::new(&None, variable.as_ref(), None)]),
            )),
            "candidates" => Candidates::from_candidates(&[variable, Box::new(concrete())]),
            "nested" => Box::new(ArrayType::new(&DomainType::new(
                &name,
                &MultirangeType::new(&name, &RangeType::new(&name, variable.as_ref())),
            ))),
            _ => unreachable!("test shape"),
        }
    }
}

fn assert_query<Catalog: TypeContextRead>(
    context: &TypeContext<'_, Catalog>,
    source: &dyn TypeDefinition,
    target: &dyn TypeDefinition,
    mode: CastContext,
) where
    Catalog::Error: std::fmt::Debug,
{
    let result = if mode == CastContext::implicit() {
        context.implicit_cast(source, target)
    } else if mode == CastContext::assignment() {
        context.assignment_cast(source, target)
    } else {
        context.explicit_cast(source, target)
    };
    let outcome = result.unwrap();
    assert!(outcome.resolution().is_none());
    let query = outcome.deferred_query().unwrap();
    assert_eq!(query.mode(), mode);
    assert_eq!(query.direction(), CastQueryDirection::pair());
    assert_eq!(query.dialect(), Dialect::postgres());
    assert!(query.source().unwrap().same_type(source));
    assert!(query.target().unwrap().same_type(target));
}

#[rstest::rstest]
fn pair_queries_preserve_unresolved_binding_without_provider_calls(
    #[values(
        C::anyelement(),
        C::anyarray(),
        C::anynonarray(),
        C::anyenum(),
        C::anyrange(),
        C::anymultirange(),
        C::anycompatible(),
        C::anycompatiblearray(),
        C::anycompatiblenonarray(),
        C::anycompatiblerange(),
        C::anycompatiblemultirange()
    )]
    category: C,
    #[values(
        CastContext::implicit(),
        CastContext::assignment(),
        CastContext::explicit()
    )]
    mode: CastContext,
) {
    // Every provider fact read fails. Deferred queries must never reach it.
    let provider = NativeCatalog::failing().with_dialect(Dialect::postgres());
    let context = TypeContext::new(&provider);
    let variable = test_helpers::variable(category);
    let distinct = test_helpers::variable(category);
    let other_family = test_helpers::variable(
        if category.family() == crate::types::PolymorphicFamily::simple() {
            C::anycompatible()
        } else {
            C::anyelement()
        },
    );
    let concrete = test_helpers::concrete();
    assert_query(&context, &concrete, variable.as_ref(), mode);
    assert_query(&context, variable.as_ref(), &concrete, mode);
    assert_query(&context, variable.as_ref(), variable.as_ref(), mode);
    assert_query(&context, variable.as_ref(), distinct.as_ref(), mode);
    assert_query(&context, variable.as_ref(), other_family.as_ref(), mode);
}

fn assert_enumerations<Catalog: TypeContextRead>(
    context: &TypeContext<'_, Catalog>,
    value: &dyn TypeDefinition,
) where
    Catalog::Error: std::fmt::Debug,
{
    let outcomes = [
        context.cast_sources(value, false).unwrap(),
        context.cast_sources(value, true).unwrap(),
        context.assignment_sources(value).unwrap(),
        context.implicit_castable(value).unwrap(),
    ];
    for (index, outcome) in outcomes.into_iter().enumerate() {
        let query = outcome.deferred_query().unwrap();
        assert!(outcome.resolution().is_none());
        assert_eq!(query.dialect(), Dialect::postgres());
        if index == 3 {
            assert_eq!(query.direction(), CastQueryDirection::targets());
            assert!(query.target().is_none());
            assert!(query.source().unwrap().same_type(value));
        } else {
            assert_eq!(query.direction(), CastQueryDirection::sources());
            assert!(query.source().is_none());
            assert!(query.target().unwrap().same_type(value));
        }
        let mode = match index {
            1 => CastContext::explicit(),
            2 => CastContext::assignment(),
            _ => CastContext::implicit(),
        };
        assert_eq!(query.mode(), mode);
    }
}

#[rstest::rstest]
fn recursive_wrappers_defer_through_native_and_legacy_public_apis(
    #[values(
        "leaf",
        "array",
        "range",
        "multirange",
        "domain",
        "composite",
        "implicit_record",
        "row",
        "candidates",
        "nested"
    )]
    shape: &str,
    #[values(
        CastContext::implicit(),
        CastContext::assignment(),
        CastContext::explicit()
    )]
    mode: CastContext,
) {
    let native = NativeCatalog::failing().with_dialect(Dialect::postgres());
    let legacy = crate::test_helpers::TestParseContext::default().with_dialect(Dialect::postgres());
    let value = test_helpers::wrapped(shape);
    let concrete = test_helpers::concrete();
    assert!(value.contains_polymorphic());
    assert_query(&TypeContext::new(&native), value.as_ref(), &concrete, mode);
    assert_query(&TypeContext::new(&legacy), &concrete, value.as_ref(), mode);
    assert_query(
        &TypeContext::new(&legacy),
        value.as_ref(),
        value.as_ref(),
        mode,
    );
    assert_enumerations(&TypeContext::new(&native), value.as_ref());
    assert_enumerations(&TypeContext::new(&legacy), value.as_ref());
}

#[rstest::rstest]
fn unsupported_dialect_is_not_absence_or_invalidity(
    #[values(
        "leaf",
        "array",
        "range",
        "multirange",
        "domain",
        "composite",
        "implicit_record",
        "row",
        "candidates",
        "nested"
    )]
    shape: &str,
    #[values(
        CastContext::implicit(),
        CastContext::assignment(),
        CastContext::explicit()
    )]
    mode: CastContext,
) {
    let native = NativeCatalog::failing().with_dialect(Dialect::sqlite());
    let context = TypeContext::new(&native);
    let value = test_helpers::wrapped(shape);
    let result = if mode == CastContext::implicit() {
        context.implicit_cast(value.as_ref(), value.as_ref())
    } else if mode == CastContext::assignment() {
        context.assignment_cast(value.as_ref(), value.as_ref())
    } else {
        context.explicit_cast(value.as_ref(), value.as_ref())
    };
    let error = result.unwrap_err();
    assert_eq!(error.unsupported_query_dialect(), Some(Dialect::sqlite()));
    assert!(error.provider_error().is_none());
    for result in [
        context.cast_sources(value.as_ref(), false),
        context.cast_sources(value.as_ref(), true),
        context.assignment_sources(value.as_ref()),
        context.implicit_castable(value.as_ref()),
    ] {
        assert_eq!(
            result.unwrap_err().unsupported_query_dialect(),
            Some(Dialect::sqlite())
        );
    }
}

#[test]
fn deferred_query_clones_keep_linked_variables_after_scope_disposal() {
    let outcome: CastQueryOutcome<Option<crate::types::CastDefinition>> = {
        let provider = NativeCatalog::postgres();
        let signature = SignaturePolymorphism::new(&[
            P::new(C::anyarray(), R::input()),
            P::new(C::anyenum(), R::output()),
        ])
        .unwrap();
        TypeContext::new(&provider)
            .implicit_cast(&signature.types()[0], &signature.types()[1])
            .unwrap()
    };
    let cloned = outcome.clone();
    let query = cloned.deferred_query().unwrap();
    let source = query
        .source()
        .unwrap()
        .as_any()
        .downcast_ref::<PolymorphicType>()
        .unwrap();
    let target = query
        .target()
        .unwrap()
        .as_any()
        .downcast_ref::<PolymorphicType>()
        .unwrap();
    assert_eq!(&source.variable().element(), target.variable());
    assert!(target.variable().requires_enum());
    assert!(source.same_type(outcome.deferred_query().unwrap().source().unwrap()));
}

#[cfg(feature = "sqlite")]
#[test]
fn legacy_sqlite_provider_rejects_postgres_constraint_operations_explicitly() {
    let legacy = crate::test_helpers::TestParseContext::default().with_dialect(Dialect::sqlite());
    let context = TypeContext::new(&legacy);
    let value = test_helpers::wrapped("nested");
    assert_eq!(
        context
            .implicit_cast(value.as_ref(), value.as_ref())
            .unwrap_err()
            .unsupported_query_dialect(),
        Some(Dialect::sqlite())
    );
    assert_eq!(
        context
            .implicit_castable(value.as_ref())
            .unwrap_err()
            .unsupported_query_dialect(),
        Some(Dialect::sqlite())
    );
}

#[test]
fn concrete_ranges_resolve_identity_without_inventing_subtype_casts() {
    let integer = test_helpers::concrete();
    let text = BaseType::new(&FullName::with_schema("pg_catalog", "text"));
    let first = RangeType::new(&FullName::with_schema("native", "first"), &integer);
    let second = RangeType::new(&FullName::with_schema("native", "second"), &text);
    let provider = NativeCatalog::io_only(&first, &second);
    let context = TypeContext::new(&provider);
    let cast = context
        .implicit_cast(&first, &first)
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert!(cast.context().is_no_conversion());
    assert!(
        context
            .implicit_cast(&first, &second)
            .unwrap()
            .into_resolution()
            .unwrap()
            .is_none()
    );
    let multi = MultirangeType::new(&FullName::with_schema("native", "multi"), &first);
    assert!(
        context
            .implicit_cast(&multi, &multi)
            .unwrap()
            .into_resolution()
            .unwrap()
            .unwrap()
            .context()
            .is_no_conversion()
    );
}
