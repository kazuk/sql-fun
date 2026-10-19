use super::{
    ConversionError, from_legacy_definition, from_legacy_reference, load_definition, load_reference,
};
use crate::sem::{FullName, TypeDefinition as LegacyDefinition, TypeReference as LegacyReference};
use crate::types::{
    CompositeType, DomainType, EnumType, ImplicitCompositeType, TableRowType, array::ArrayType,
    base::BaseType,
};

mod test_helpers {
    use crate::sem::{
        ColumnCollection, ColumnDefinition, ColumnName, CompositeFieldCollection,
        CompositeFieldDefinition, CreateCompositeType, CreateDomain, CreateEnum, CreateTable,
        CreateTableMode, FullName, SchemaName, TableName, TypeDefinition, TypeReference,
    };
    use crate::test_helpers::{TestParseContext, source_span};

    pub(super) fn context() -> TestParseContext {
        let mut context = TestParseContext::default().with_dialect(crate::ast::Dialect::postgres());
        context.set_get_search_path_result(&[SchemaName::from("public")]);
        context
    }

    pub(super) fn name(local: &str) -> FullName {
        FullName::with_schema("public", local)
    }

    pub(super) fn enumeration(name: &FullName) -> TypeDefinition {
        TypeDefinition::Enum(CreateEnum::new(
            name,
            &["first".to_owned(), "Second's".to_owned()],
        ))
    }

    pub(super) fn domain(name: &FullName, base: Option<&FullName>) -> TypeDefinition {
        TypeDefinition::Domain(CreateDomain::new(
            &TypeReference::concrete_type_ref(name, false),
            &base.cloned(),
        ))
    }

    pub(super) fn composite(name: &FullName, reference: Option<&TypeReference>) -> TypeDefinition {
        let mut fields = CompositeFieldCollection::default();
        fields.push(CompositeFieldDefinition::new(
            "Value.Name",
            &reference.cloned(),
            &Some(true),
        ));
        TypeDefinition::CompositeType(CreateCompositeType::new(name, &fields))
    }

    pub(super) fn composite_with_identity(
        name: &FullName,
        identity: &TypeReference,
    ) -> TypeDefinition {
        let composite = CreateCompositeType::new(name, &CompositeFieldCollection::default());
        // Serialized declarations can contain identity references that the
        // ordinary constructor does not produce.
        let mut serialized = serde_json::to_value(composite).unwrap();
        serialized["type_ref"] = serde_json::to_value(identity).unwrap();
        TypeDefinition::CompositeType(serde_json::from_value(serialized).unwrap())
    }

    pub(super) fn columns(reference: Option<&TypeReference>) -> ColumnCollection {
        ColumnCollection::new(vec![ColumnDefinition::new(
            &Some(ColumnName::new("Value.Name")),
            reference,
            Some(true),
            &[],
        )])
    }

    pub(super) fn implicit(reference: Option<&TypeReference>) -> TypeDefinition {
        TypeDefinition::ImplicitComposite(columns(reference))
    }

    pub(super) fn table(
        context: &mut TestParseContext,
        name: &FullName,
        reference: &TypeReference,
    ) -> TypeDefinition {
        let table_name = TableName::from(name.clone());
        let definition = CreateTable::new(
            &table_name,
            vec![ColumnDefinition::new(
                &Some(ColumnName::new("Value.Name")),
                Some(reference),
                Some(true),
                &[],
            )],
            CreateTableMode::Default,
            false,
            &source_span(),
        );
        context.set_get_table_impl_result(name, &Some(definition));
        TypeDefinition::table_ref(&table_name)
    }
}

#[rstest::rstest]
fn converts_builtin_definition(#[values(false, true)] array: bool) {
    let name = FullName::with_catalog("My.Db", "Mixed.Schema", "Type.Name[]");
    let definition = LegacyDefinition::Builtin(LegacyReference::concrete_type_ref(&name, array));
    let loaded = from_legacy_definition(&definition).unwrap();
    let terminal = if array {
        loaded
            .as_any()
            .downcast_ref::<ArrayType>()
            .unwrap()
            .element_type()
    } else {
        loaded.as_ref()
    };
    assert_eq!(
        terminal.as_any().downcast_ref::<BaseType>().unwrap().name(),
        &name
    );
}

#[test]
fn nested_dynamic_definition_preserves_enum_payload() {
    let name = FullName::with_catalog("My.Db", "Mixed.Schema", "State.Name");
    let enumeration = test_helpers::enumeration(&name);
    let nested = LegacyDefinition::Builtin(LegacyReference::dynamic(&LegacyDefinition::Builtin(
        LegacyReference::dynamic(&enumeration),
    )));
    let loaded = from_legacy_reference(&LegacyReference::dynamic(&nested)).unwrap();
    let enumeration = loaded.as_any().downcast_ref::<EnumType>().unwrap();
    assert_eq!(enumeration.name(), &name);
    assert_eq!(enumeration.labels(), &["first", "Second's"]);
}

#[rstest::rstest]
fn named_reference_requires_catalog(#[values(false, true)] array: bool) {
    let name = test_helpers::name("state");
    let reference = LegacyReference::concrete_type_ref(&name, array);
    assert_eq!(
        from_legacy_reference(&reference).unwrap_err(),
        ConversionError::catalog_required(&name)
    );
}

#[rstest::rstest]
fn loads_enum_and_array_of_enum(#[values(false, true)] array: bool) {
    let mut context = test_helpers::context();
    let name = test_helpers::name("State.Name");
    context.set_get_type_impl_result(&name, &test_helpers::enumeration(&name));
    let loaded =
        load_reference(&context, &LegacyReference::concrete_type_ref(&name, array)).unwrap();
    let element = if array {
        loaded
            .as_any()
            .downcast_ref::<ArrayType>()
            .unwrap()
            .element_type()
    } else {
        loaded.as_ref()
    };
    assert_eq!(
        element.as_any().downcast_ref::<EnumType>().unwrap().name(),
        &name
    );
}

#[test]
fn resolves_unqualified_name_using_selected_context() {
    let mut context = test_helpers::context();
    let qualified = test_helpers::name("state");
    context.set_get_type_impl_result(&qualified, &test_helpers::enumeration(&qualified));
    let loaded = load_reference(
        &context,
        &LegacyReference::concrete_type_ref(&FullName::from_local_name("state"), false),
    )
    .unwrap();
    assert_eq!(
        loaded.as_any().downcast_ref::<EnumType>().unwrap().name(),
        &qualified
    );
}

#[test]
fn loads_domain_with_non_base_underlying_type() {
    let mut context = test_helpers::context();
    let base_name = test_helpers::name("state");
    let domain_name = test_helpers::name("status");
    context.set_get_type_impl_result(&base_name, &test_helpers::enumeration(&base_name));
    let definition = test_helpers::domain(&domain_name, Some(&base_name));
    let loaded = load_definition(&context, &definition).unwrap();
    let domain = loaded.as_any().downcast_ref::<DomainType>().unwrap();
    assert_eq!(domain.name(), &domain_name);
    assert_eq!(
        domain
            .base_type()
            .as_any()
            .downcast_ref::<EnumType>()
            .unwrap()
            .name(),
        &base_name
    );
    assert_eq!(
        from_legacy_definition(&definition).unwrap_err(),
        ConversionError::catalog_required(&base_name)
    );
}

#[test]
fn loads_composite_members_through_context() {
    let mut context = test_helpers::context();
    let name = test_helpers::name("record");
    let enum_name = test_helpers::name("state");
    context.set_get_type_impl_result(&enum_name, &test_helpers::enumeration(&enum_name));
    let reference = LegacyReference::concrete_type_ref(&enum_name, true);
    let definition = test_helpers::composite(&name, Some(&reference));
    let loaded = load_definition(&context, &definition).unwrap();
    let composite = loaded.as_any().downcast_ref::<CompositeType>().unwrap();
    assert_eq!(composite.name(), Some(&name));
    let member = composite.members().member("Value.Name").unwrap();
    assert_eq!(member.is_not_null(), Some(true));
    assert!(member.member_type().as_any().is::<ArrayType>());
    assert_eq!(
        from_legacy_definition(&definition).unwrap_err(),
        ConversionError::catalog_required(&enum_name)
    );
}

#[test]
fn converts_dynamic_implicit_composite_without_catalog() {
    let enumeration = test_helpers::enumeration(&test_helpers::name("state"));
    let definition = test_helpers::implicit(Some(&LegacyReference::dynamic(&enumeration)));
    let loaded = from_legacy_definition(&definition).unwrap();
    let implicit = loaded
        .as_any()
        .downcast_ref::<ImplicitCompositeType>()
        .unwrap();
    let member = implicit.members().member("Value.Name").unwrap();
    assert_eq!(member.is_not_null(), Some(true));
    assert!(member.member_type().as_any().is::<EnumType>());
}

#[test]
fn loads_table_row_members_and_requires_catalog_structurally() {
    let mut context = test_helpers::context();
    let name = test_helpers::name("Table.Name");
    let enum_name = test_helpers::name("state");
    context.set_get_type_impl_result(&enum_name, &test_helpers::enumeration(&enum_name));
    let definition = test_helpers::table(
        &mut context,
        &name,
        &LegacyReference::concrete_type_ref(&enum_name, false),
    );
    let loaded = load_definition(&context, &definition).unwrap();
    let row = loaded.as_any().downcast_ref::<TableRowType>().unwrap();
    assert_eq!(row.name(), &name);
    assert!(
        row.members()
            .member("Value.Name")
            .unwrap()
            .member_type()
            .as_any()
            .is::<EnumType>()
    );
    assert_eq!(
        from_legacy_definition(&definition).unwrap_err(),
        ConversionError::catalog_required(&name)
    );
}

#[test]
fn reports_missing_domain_base() {
    let name = test_helpers::name("incomplete");
    let definition = test_helpers::domain(&name, None);
    assert_eq!(
        from_legacy_definition(&definition).unwrap_err(),
        ConversionError::missing_domain_base(&name)
    );
}

#[rstest::rstest]
#[case::implicit(false)]
#[case::explicit(true)]
fn reports_missing_member_type(#[case] explicit: bool) {
    let definition = if explicit {
        test_helpers::composite(&test_helpers::name("record"), None)
    } else {
        test_helpers::implicit(None)
    };
    assert_eq!(
        from_legacy_definition(&definition).unwrap_err(),
        ConversionError::missing_member_type(0)
    );
}

#[test]
fn reports_missing_named_type() {
    let context = test_helpers::context();
    let name = test_helpers::name("missing");
    assert_eq!(
        load_reference(&context, &LegacyReference::concrete_type_ref(&name, false)).unwrap_err(),
        ConversionError::type_not_found(&name)
    );
}

#[test]
fn reports_missing_relation() {
    let context = test_helpers::context();
    let name = test_helpers::name("missing");
    let definition = LegacyDefinition::table_ref(&crate::sem::TableName::from(name.clone()));
    assert_eq!(
        load_definition(&context, &definition).unwrap_err(),
        ConversionError::relation_not_found(&name)
    );
}

#[rstest::rstest]
fn rejects_recursive_domains(#[values(false, true)] indirect: bool) {
    let mut context = test_helpers::context();
    let first = test_helpers::name("first");
    let second = test_helpers::name("second");
    let base = if indirect { &second } else { &first };
    context.set_get_type_impl_result(&first, &test_helpers::domain(&first, Some(base)));
    context.set_get_type_impl_result(&second, &test_helpers::domain(&second, Some(&first)));
    assert_eq!(
        load_reference(&context, &LegacyReference::concrete_type_ref(&first, false)).unwrap_err(),
        ConversionError::recursive_type(&first)
    );
}

#[rstest::rstest]
#[case::range("int4range")]
#[case::multirange("int4multirange")]
#[case::polymorphic("anyelement")]
#[case::compatible("anycompatible")]
#[case::record("record")]
#[case::pseudo("void")]
fn rejects_named_categories_without_components(#[case] local: &str) {
    let name = FullName::with_schema("pg_catalog", local);
    assert_eq!(
        from_legacy_definition(&LegacyDefinition::builtin(&name)).unwrap_err(),
        ConversionError::unsupported_named_type(&name)
    );
}

#[rstest::rstest]
#[case::enumeration(false)]
#[case::builtin(true)]
fn retains_resolved_catalog_key_for_unqualified_declaration(#[case] builtin: bool) {
    let mut context = test_helpers::context();
    let local = FullName::from_local_name("state");
    let public = FullName::with_schema("public", "state");
    let other = FullName::with_schema("other", "state");
    let declaration = if builtin {
        LegacyDefinition::builtin(&local)
    } else {
        test_helpers::enumeration(&local)
    };
    context.set_get_type_impl_result(&public, &declaration);
    context.set_get_type_impl_result(&other, &declaration);
    let public_type = load_reference(
        &context,
        &LegacyReference::concrete_type_ref(&public, false),
    )
    .unwrap();
    let other_type =
        load_reference(&context, &LegacyReference::concrete_type_ref(&other, false)).unwrap();
    assert!(!public_type.same_type(other_type.as_ref()));
    if builtin {
        assert_eq!(
            public_type
                .as_any()
                .downcast_ref::<BaseType>()
                .unwrap()
                .name(),
            &public
        );
        assert_eq!(
            other_type
                .as_any()
                .downcast_ref::<BaseType>()
                .unwrap()
                .name(),
            &other
        );
    } else {
        assert_eq!(
            public_type
                .as_any()
                .downcast_ref::<EnumType>()
                .unwrap()
                .name(),
            &public
        );
        assert_eq!(
            other_type
                .as_any()
                .downcast_ref::<EnumType>()
                .unwrap()
                .name(),
            &other
        );
    }
}

#[test]
fn rejects_structural_nesting_beyond_conversion_limit() {
    let mut definition = LegacyDefinition::builtin(&test_helpers::name("terminal"));
    for _ in 0..129 {
        definition = LegacyDefinition::Builtin(LegacyReference::dynamic(&definition));
    }
    assert_eq!(
        from_legacy_definition(&definition).unwrap_err(),
        ConversionError::depth_limit()
    );
}

#[test]
fn preserves_table_row_array_qualifier() {
    let mut context = test_helpers::context();
    let name = test_helpers::name("items");
    let enumeration = test_helpers::enumeration(&test_helpers::name("state"));
    test_helpers::table(&mut context, &name, &LegacyReference::dynamic(&enumeration));
    let definition = LegacyDefinition::TableRef(
        crate::sem::TableName::from(name.clone()),
        LegacyReference::concrete_type_ref(&name, true),
    );
    let loaded = load_definition(&context, &definition).unwrap();
    let array = loaded.as_any().downcast_ref::<ArrayType>().unwrap();
    let row = array
        .element_type()
        .as_any()
        .downcast_ref::<TableRowType>()
        .unwrap();
    assert_eq!(row.name(), &name);
    assert!(
        row.members()
            .member("Value.Name")
            .unwrap()
            .member_type()
            .as_any()
            .is::<EnumType>()
    );
}

#[rstest::rstest]
#[case::dynamic_payload(true)]
#[case::mismatched_name(false)]
fn rejects_table_references_with_unrepresentable_payload(#[case] dynamic: bool) {
    let mut context = test_helpers::context();
    let name = test_helpers::name("items");
    let other = test_helpers::name("other");
    let enumeration = test_helpers::enumeration(&test_helpers::name("state"));
    test_helpers::table(&mut context, &name, &LegacyReference::dynamic(&enumeration));
    let reference = if dynamic {
        LegacyReference::dynamic(&enumeration)
    } else {
        LegacyReference::concrete_type_ref(&other, false)
    };
    let definition =
        LegacyDefinition::TableRef(crate::sem::TableName::from(name.clone()), reference);
    let unsupported = if dynamic { &name } else { &other };
    assert_eq!(
        load_definition(&context, &definition).unwrap_err(),
        ConversionError::unsupported_named_type(unsupported)
    );
}

#[test]
fn preserves_domain_identity_array_qualifier() {
    let mut context = test_helpers::context();
    let name = test_helpers::name("statuses");
    let base = test_helpers::name("state");
    context.set_get_type_impl_result(&base, &test_helpers::enumeration(&base));
    let definition = LegacyDefinition::Domain(crate::sem::CreateDomain::new(
        &LegacyReference::concrete_type_ref(&name, true),
        &Some(base.clone()),
    ));
    let loaded = load_definition(&context, &definition).unwrap();
    let array = loaded.as_any().downcast_ref::<ArrayType>().unwrap();
    let domain = array
        .element_type()
        .as_any()
        .downcast_ref::<DomainType>()
        .unwrap();
    assert_eq!(domain.name(), &name);
    assert_eq!(
        domain
            .base_type()
            .as_any()
            .downcast_ref::<EnumType>()
            .unwrap()
            .name(),
        &base
    );
}

#[test]
fn rejects_dynamic_domain_identity_without_panicking() {
    let base = test_helpers::name("state");
    let identity = LegacyReference::dynamic(&test_helpers::enumeration(&base));
    let definition =
        LegacyDefinition::Domain(crate::sem::CreateDomain::new(&identity, &Some(base)));
    assert_eq!(
        from_legacy_definition(&definition).unwrap_err(),
        ConversionError::unsupported_identity_reference()
    );
}

#[rstest::rstest]
fn preserves_composite_identity_or_rejects_dynamic_reference(#[values(false, true)] dynamic: bool) {
    let name = test_helpers::name("records");
    let identity = if dynamic {
        LegacyReference::dynamic(&test_helpers::enumeration(&test_helpers::name("state")))
    } else {
        LegacyReference::concrete_type_ref(&name, true)
    };
    let definition = test_helpers::composite_with_identity(&name, &identity);
    let result = from_legacy_definition(&definition);
    if dynamic {
        assert_eq!(
            result.unwrap_err(),
            ConversionError::unsupported_identity_reference()
        );
    } else {
        let loaded = result.unwrap();
        let array = loaded.as_any().downcast_ref::<ArrayType>().unwrap();
        assert_eq!(
            array
                .element_type()
                .as_any()
                .downcast_ref::<CompositeType>()
                .unwrap()
                .name(),
            Some(&name)
        );
    }
}

#[test]
fn converts_anonymous_explicit_composite_without_catalog() {
    let enumeration = test_helpers::enumeration(&test_helpers::name("state"));
    let mut fields = crate::sem::CompositeFieldCollection::default();
    fields.push(crate::sem::CompositeFieldDefinition::new(
        "value",
        &Some(LegacyReference::dynamic(&enumeration)),
        &None,
    ));
    let definition =
        LegacyDefinition::CompositeType(crate::sem::CreateCompositeType::from_fields(&fields));
    let loaded = from_legacy_definition(&definition).unwrap();
    let composite = loaded.as_any().downcast_ref::<CompositeType>().unwrap();
    assert_eq!(composite.name(), None);
    assert!(
        composite
            .members()
            .member("value")
            .unwrap()
            .member_type()
            .as_any()
            .is::<EnumType>()
    );
}

#[test]
fn explicitly_converts_definition_retrieved_from_context() {
    use crate::sem::BaseParseContext;

    let mut context = test_helpers::context();
    let name = test_helpers::name("state");
    context.set_get_type_impl_result(&name, &test_helpers::enumeration(&name));
    let legacy = context.get_type(&name).expect("selected catalog type");
    let loaded = load_definition(&context, legacy).unwrap();
    assert_eq!(
        loaded.as_any().downcast_ref::<EnumType>().unwrap().name(),
        &name
    );
}

#[test]
fn rejects_unresolved_unqualified_definition_name() {
    let context = test_helpers::context();
    let name = FullName::from_local_name("missing");
    assert_eq!(
        load_definition(&context, &test_helpers::enumeration(&name)).unwrap_err(),
        ConversionError::type_not_found(&name)
    );
}
