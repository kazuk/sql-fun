use super::{
    PolymorphicCategory as C, PolymorphicFamily as F, PolymorphicPosition as P,
    PolymorphicRole as R, SignaturePolymorphism,
};
use crate::sem::{FullName, TypeReference as LegacyReference};
use crate::types::{Any, Candidates, Never, TypeDefinition, base::BaseType};

#[rstest::rstest]
#[case::element("anyelement", C::anyelement(), F::simple())]
#[case::array("anyarray", C::anyarray(), F::simple())]
#[case::nonarray("anynonarray", C::anynonarray(), F::simple())]
#[case::enumeration("anyenum", C::anyenum(), F::simple())]
#[case::range("anyrange", C::anyrange(), F::simple())]
#[case::multirange("anymultirange", C::anymultirange(), F::simple())]
#[case::compatible("anycompatible", C::anycompatible(), F::common())]
#[case::compatible_array("anycompatiblearray", C::anycompatiblearray(), F::common())]
#[case::compatible_nonarray("anycompatiblenonarray", C::anycompatiblenonarray(), F::common())]
#[case::compatible_range("anycompatiblerange", C::anycompatiblerange(), F::common())]
#[case::compatible_multirange("anycompatiblemultirange", C::anycompatiblemultirange(), F::common())]
fn every_category_constructs_without_catalog(
    #[case] spelling: &str,
    #[case] category: C,
    #[case] family: F,
) {
    assert_eq!(C::from_postgres_identifier(spelling), Some(category));
    assert_eq!(category.spelling(), spelling);
    assert_eq!(category.family(), family);
    let name = FullName::with_schema("pg_catalog", spelling);
    assert_eq!(C::from_postgres_name(&name), Some(category));
    let legacy = LegacyReference::from_full_name(name);
    let position = crate::types::compat::polymorphic_position(&legacy, R::input()).unwrap();
    let signature = SignaturePolymorphism::new(&[position, P::new(category, R::output())]).unwrap();
    let value = &signature.types()[0];
    assert!(value.same_type(&signature.types()[1]));
    assert!(value.contains_polymorphic());
    assert!(value.same_type(value.clone_box().as_ref()));
    assert!(!value.same_type(&Any::new()));
    assert!(!value.same_type(&Never::new()));
    assert!(!value.same_type(Candidates::from_candidates(&[value.clone_box()]).as_ref()));
    assert!(!value.same_type(&BaseType::new(&FullName::with_schema(
        "pg_catalog",
        "unknown"
    ))));
}

#[rstest::rstest]
#[case::any("any")]
#[case::unknown("unknown")]
#[case::missing_enum("anycompatibleenum")]
#[case::case_sensitive("ANYELEMENT")]
#[case::raw_quotes(r#""anyelement""#)]
#[case::raw_qualified("pg_catalog.anyelement")]
fn rejects_nonlogical_or_nonpolymorphic_identifiers(#[case] spelling: &str) {
    assert_eq!(C::from_postgres_identifier(spelling), None);
}

#[rstest::rstest]
#[case::user_schema(FullName::with_schema("public", "anyelement"))]
#[case::unresolved(FullName::from_local_name("anyelement"))]
fn legacy_adapter_never_resolves_ambient_names(#[case] name: FullName) {
    let reference = LegacyReference::from_full_name(name);
    assert!(crate::types::compat::polymorphic_position(&reference, R::input()).is_none());
}

#[test]
fn restrictions_and_relations_are_shared_including_outputs() {
    let signature = SignaturePolymorphism::new(&[
        P::new(C::anyarray(), R::input()),
        P::new(C::anyelement(), R::input()),
        P::new(C::anyenum(), R::output()),
        P::new(C::anyrange(), R::input()),
        P::new(C::anymultirange(), R::output()),
        P::new(C::anynonarray(), R::in_out()),
        P::new(C::anycompatiblearray(), R::variadic()),
        P::new(C::anycompatible(), R::output()),
    ])
    .unwrap();
    let values = signature.types();
    let element = values[0].variable().element();
    assert_eq!(&element, values[1].variable());
    assert_eq!(&element, values[2].variable());
    assert_eq!(values[0].variable(), &element.array());
    assert_eq!(values[3].variable(), &element.range());
    assert_eq!(values[4].variable(), &values[3].variable().multirange());
    assert!(values[1].variable().requires_enum());
    assert!(values[5].variable().requires_nonarray());
    assert!(!values[6].variable().requires_enum());
    assert!(!values[6].variable().requires_nonarray());
    assert_ne!(element, values[7].variable().element());
    assert_eq!(&values[6].variable().element(), values[7].variable());
    assert_eq!(signature.positions()[6].role(), R::variadic());
}

#[test]
fn scope_identity_survives_cloning_and_source_disposal() {
    let positions = [
        P::new(C::anyelement(), R::input()),
        P::new(C::anyelement(), R::output()),
    ];
    let first = SignaturePolymorphism::new(&positions).unwrap();
    let independent = SignaturePolymorphism::new(&positions).unwrap();
    let clone = first.clone();
    let value = first.types()[0].clone_box();
    drop(first);
    let third = value.clone();
    assert!(value.same_type(&clone.types()[0]));
    assert!(clone.types()[0].same_type(third.as_ref()));
    assert!(third.same_type(value.as_ref()));
    assert!(!value.same_type(&independent.types()[0]));
}

#[rstest::rstest]
#[case::simple(C::anymultirange(), C::anyrange())]
#[case::common(C::anycompatiblemultirange(), C::anycompatiblerange())]
#[case::simple_reverse(C::anyrange(), C::anymultirange())]
#[case::common_reverse(C::anycompatiblerange(), C::anycompatiblemultirange())]
fn range_and_multirange_supply_each_others_identity(#[case] input: C, #[case] output: C) {
    let signature =
        SignaturePolymorphism::new(&[P::new(input, R::input()), P::new(output, R::output())])
            .unwrap();
    assert_eq!(
        signature.types()[0].variable().range(),
        signature.types()[1].variable().range()
    );
}

#[rstest::rstest]
#[case::missing_input(vec![P::new(C::anyelement(), R::output())])]
#[case::wrong_family(vec![P::new(C::anyelement(), R::input()), P::new(C::anycompatible(), R::output())])]
#[case::simple_range(vec![P::new(C::anyelement(), R::input()), P::new(C::anyrange(), R::output())])]
#[case::common_range(vec![P::new(C::anycompatiblearray(), R::input()), P::new(C::anycompatiblemultirange(), R::output())])]
#[case::variadic(vec![P::new(C::anyelement(), R::variadic())])]
fn malformed_declarations_are_rejected_atomically(#[case] positions: Vec<P>) {
    assert!(SignaturePolymorphism::new(&positions).is_err());
}

#[test]
fn legacy_array_flags_are_not_named_array_pseudotypes() {
    let reference = LegacyReference::concrete_type_ref(
        &FullName::with_schema("pg_catalog", "anyelement"),
        true,
    );
    assert!(crate::types::compat::polymorphic_position(&reference, R::input()).is_none());
}

#[test]
fn output_nonarray_restriction_applies_to_common_input_elements() {
    let signature = SignaturePolymorphism::new(&[
        P::new(C::anycompatiblearray(), R::input()),
        P::new(C::anycompatiblenonarray(), R::output()),
    ])
    .unwrap();
    assert_eq!(
        signature.types()[0].variable().element(),
        *signature.types()[1].variable()
    );
    assert!(signature.types()[0].variable().requires_nonarray());
    assert!(!signature.types()[0].variable().requires_enum());
}

#[rstest::rstest]
#[case::simple(C::anyelement(), R::input(), C::anyarray())]
#[case::common(C::anycompatible(), R::input(), C::anycompatiblearray())]
#[case::simple_variadic(C::anyarray(), R::variadic(), C::anyelement())]
#[case::common_variadic(C::anycompatiblearray(), R::variadic(), C::anycompatible())]
fn array_and_variadic_relations_do_not_invent_nonarray_restrictions(
    #[case] input: C,
    #[case] role: R,
    #[case] output: C,
) {
    // PostgreSQL accepts domains over arrays as array elements, including
    // variadic calls. Explicit anynonarray has a different restriction.
    let signature =
        SignaturePolymorphism::new(&[P::new(input, role), P::new(output, R::output())]).unwrap();
    assert!(!signature.types()[0].variable().requires_nonarray());
    assert_eq!(
        signature.types()[0].variable().element(),
        signature.types()[1].variable().element()
    );
    assert_eq!(
        signature.positions()[0].role().is_variadic(),
        role == R::variadic()
    );
}
