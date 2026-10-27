use crate::{
    sem::{
        ColumnCollection, CompositeFieldCollection, CreateCompositeType, CreateDomain, CreateEnum,
        FullName, TableName, TypeDefinition, TypeReference,
    },
    test_helpers::TestParseContext,
};

#[rstest::rstest]
fn retained_fact_is_used_without_name_or_element_inference(
    #[values(Some(true), Some(false), None)] fact: Option<bool>,
    #[values(false, true)] dynamic: bool,
    #[values(false, true)] array: bool,
) {
    let mut context = TestParseContext::default();
    // A qualified, vector-like name must not determine the answer.
    let name = FullName::with_schema("extension schema", "_custom_vector");
    let definition = TypeDefinition::builtin(&name, fact);
    context.set_get_type_impl_result(&name, &definition);
    let reference = if dynamic {
        TypeReference::dynamic(&definition)
    } else {
        TypeReference::concrete_type_ref(&name, false)
    };
    let reference = if array {
        reference.array_type()
    } else {
        reference
    };
    assert_eq!(reference.collation_capability(&context), fact);
    assert_eq!(context.reported_problem_count(), 0);
}

#[rstest::rstest]
fn follows_nested_domains_and_arrays(
    #[values(Some(true), Some(false), None)] fact: Option<bool>,
    #[values(false, true)] array_base: bool,
    #[values(false, true)] array_domain: bool,
    #[values(false, true)] dynamic: bool,
) {
    let mut context = TestParseContext::default();
    let base_name = FullName::with_schema("extension", "base");
    let base = TypeReference::concrete_type_ref(&base_name, array_base);
    context.set_get_type_impl_result(&base_name, &TypeDefinition::builtin(&base_name, fact));
    let first = TypeReference::concrete_type_ref(&FullName::with_schema("public", "first"), false);
    let second =
        TypeReference::concrete_type_ref(&FullName::with_schema("public", "second"), false);
    let first_definition = TypeDefinition::from(&CreateDomain::new(&first, &Some(base)));
    context.set_get_type_impl_result(first.full_name(), &first_definition);
    let second_definition = TypeDefinition::from(&CreateDomain::new(&second, &Some(first)));
    context.set_get_type_impl_result(second.full_name(), &second_definition);
    let reference = if dynamic {
        TypeReference::dynamic(&second_definition)
    } else {
        second
    };
    let reference = if array_domain {
        reference.array_type()
    } else {
        reference
    };
    assert_eq!(reference.collation_capability(&context), fact);
}

#[rstest::rstest]
#[case::missing_base(None)]
#[case::missing_definition(Some("missing"))]
#[case::cycle(Some("cycle"))]
fn unresolved_and_cyclic_domains_have_no_fact(#[case] base: Option<&str>) {
    let mut context = TestParseContext::default();
    let reference =
        TypeReference::concrete_type_ref(&FullName::with_schema("public", "cycle"), false);
    let base = base.map(|name| {
        TypeReference::concrete_type_ref(&FullName::with_schema("public", name), false)
    });
    let domain = TypeDefinition::from(&CreateDomain::new(&reference, &base));
    context.set_get_type_impl_result(reference.full_name(), &domain);
    assert_eq!(reference.collation_capability(&context), None);
    assert_eq!(
        TypeReference::dynamic(&domain).collation_capability(&context),
        None
    );
}

#[test]
fn structural_rows_are_not_collatable() {
    let context = TestParseContext::default();
    let composite = CreateCompositeType::from_fields(&CompositeFieldCollection::default());
    let reference = TypeReference::dynamic(&TypeDefinition::from(&composite));
    assert_eq!(reference.collation_capability(&context), Some(false));
    let row = TypeReference::dynamic(&TypeDefinition::implicit_composite(
        &ColumnCollection::default(),
    ));
    assert_eq!(row.collation_capability(&context), Some(false));
    let enumeration = CreateEnum::new(&FullName::with_schema("public", "enum"), &["a".to_owned()]);
    let enumeration = TypeReference::dynamic(&TypeDefinition::from(&enumeration));
    assert_eq!(enumeration.collation_capability(&context), Some(false));
    let table = TableName::from(FullName::with_schema("public", "table"));
    let row = TypeReference::dynamic(&TypeDefinition::table_ref(&table));
    assert_eq!(row.collation_capability(&context), Some(false));
}

#[test]
#[cfg(feature = "sqlite")]
fn other_dialects_do_not_expose_postgres_facts() {
    let context = TestParseContext::default().with_dialect(crate::ast::Dialect::sqlite());
    let reference = TypeReference::dynamic(&TypeDefinition::builtin(
        &FullName::with_schema("pg_catalog", "text"),
        Some(true),
    ));
    assert_eq!(reference.collation_capability(&context), None);
}
