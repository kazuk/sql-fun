use super::BuiltinTypeInfo;
use crate::context::BaseParseContext;
use crate::sem::{FullName, TypeDefinition, TypeReference};
use crate::test_helpers::TestParseContext;

#[rstest::rstest]
#[case::seventeen("17")]
#[case::eighteen("18")]
fn committed_catalogs_survive_loading_and_lookup(#[case] version: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sql-fun-home/postgres")
        .join(version)
        .join("pg_type.json");
    let catalog = std::fs::read_to_string(path).unwrap();
    let records: Vec<BuiltinTypeInfo> = serde_json::from_str(&catalog).unwrap();
    assert!(!records.is_empty());
    let mut context = TestParseContext::default();
    for record in records {
        assert!(
            record.category.is_some(),
            "defined builtin: {}",
            record.name
        );
        let definition = TypeDefinition::from(&record);
        let name = FullName::with_schema(&record.schema, &record.name);
        context.set_get_type_impl_result(&name, &definition);
        for lookup in [name, FullName::from_local_name(&record.name)] {
            let loaded = context.get_type(&lookup).expect("type lookup");
            assert_eq!(loaded.category(&context), record.category);
            assert_eq!(
                TypeReference::dynamic(loaded).collation_capability(&context),
                record.collatable,
            );
            let json = serde_json::to_string(loaded).unwrap();
            let restored: TypeDefinition = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, *loaded);
            assert_eq!(restored.category(&context), record.category);
        }
    }
}

#[rstest::rstest]
#[case::shell(
    r#"{"schema":"pg_catalog","collatable":null,"name":"custom","category":null}"#,
    None
)]
#[case::custom(
    r#"{"schema":"pg_catalog","collatable":null,"name":"custom","category":"!"}"#,
    Some('!')
)]
#[case::unknown(
    r#"{"schema":"pg_catalog","collatable":null,"name":"unknown","category":"X"}"#,
    Some('X')
)]
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
fn definition_preserves_flattened_reference_shape() {
    let name = FullName::with_schema("pg_catalog", "custom");
    let definition = TypeDefinition::builtin(&name, None);
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
                None,
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
    let definition = TypeDefinition::builtin_with_category(&name, Some(category), None);
    let context = TestParseContext::default().with_dialect(crate::ast::Dialect::sqlite());
    assert_eq!(definition.category(&context), None);
}
