use super::BuiltinTypeInfo;
use crate::sem::{BaseParseContext, FullName, TypeDefinition};
use crate::test_helpers::TestParseContext;

#[rstest::rstest]
#[case::fifteen(include_str!("../../../tests/fixtures/type_categories/15.json"))]
#[case::sixteen(include_str!("../../../tests/fixtures/type_categories/16.json"))]
#[case::seventeen(include_str!("../../../tests/fixtures/type_categories/17.json"))]
#[case::eighteen(include_str!("../../../tests/fixtures/type_categories/18.json"))]
fn oracle_samples_survive_loading_and_lookup(#[case] fixture: &str) {
    #[derive(serde::Deserialize)]
    struct Sample {
        types: Vec<BuiltinTypeInfo>,
    }
    let sample: Sample = serde_json::from_str(fixture).unwrap();
    let mut context = TestParseContext::default();
    for record in sample.types {
        let definition = TypeDefinition::from(&record);
        let name = FullName::with_schema("pg_catalog", &record.name);
        context.set_get_type_impl_result(&name, &definition);
        for lookup in [name, FullName::from_local_name(&record.name)] {
            let loaded = context.get_type(&lookup).expect("type lookup");
            assert_eq!(loaded.category(&context), record.category);
            let json = serde_json::to_string(loaded).unwrap();
            let restored: TypeDefinition = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, *loaded);
            assert_eq!(restored.category(&context), record.category);
        }
    }
}

#[rstest::rstest]
#[case::legacy(r#"{"name":"custom"}"#, None)]
#[case::shell(r#"{"name":"custom","category":null}"#, None)]
#[case::custom(r#"{"name":"custom","category":"!"}"#, Some('!'))]
#[case::unknown(r#"{"name":"unknown","category":"X"}"#, Some('X'))]
fn preserves_available_and_missing_information(#[case] json: &str, #[case] expected: Option<char>) {
    let record: BuiltinTypeInfo = serde_json::from_str(json).unwrap();
    let definition = TypeDefinition::from(&record);
    let context = TestParseContext::default();
    assert_eq!(
        definition
            .category(&context)
            .map(crate::sem::TypeCategory::code),
        expected
    );
    let json = serde_json::to_string(&definition).unwrap();
    let restored: TypeDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.category(&context), definition.category(&context));
}

#[test]
fn legacy_definition_shape_remains_readable() {
    let name = FullName::with_schema("pg_catalog", "custom");
    let definition = TypeDefinition::builtin(&name);
    let json = serde_json::to_value(&definition).unwrap();
    assert!(json["Builtin"]["Concrete"].is_object());
    assert!(json["Builtin"].get("category").is_none());
    assert_eq!(
        serde_json::from_value::<TypeDefinition>(json).unwrap(),
        definition
    );
}

#[test]
fn category_follows_selected_definition_and_search_path() {
    use crate::sem::{SchemaName, TypeCategory};

    let mut context = TestParseContext::default();
    context.set_get_search_path_result(&[SchemaName::from("second"), SchemaName::from("first")]);
    for (schema, code) in [("first", 'q'), ("second", '!')] {
        let name = FullName::with_schema(schema, "custom");
        context.set_get_type_impl_result(
            &name,
            &TypeDefinition::builtin_with_category(
                &name,
                Some(TypeCategory::try_from(code).unwrap()),
            ),
        );
    }
    let selected = context
        .get_type(&FullName::from_local_name("custom"))
        .unwrap();
    assert_eq!(selected.category(&context).unwrap().code(), '!');
    let qualified = context
        .get_type(&FullName::with_schema("first", "custom"))
        .unwrap();
    assert_eq!(qualified.category(&context).unwrap().code(), 'q');
}

#[test]
#[cfg(feature = "sqlite")]
fn postgres_metadata_does_not_classify_sqlite_types() {
    let name = FullName::with_schema("pg_catalog", "int4");
    let category = crate::sem::TypeCategory::try_from('N').unwrap();
    let definition = TypeDefinition::builtin_with_category(&name, Some(category));
    let context = TestParseContext::default().with_dialect(crate::ast::Dialect::sqlite());
    assert_eq!(definition.category(&context), None);
}
