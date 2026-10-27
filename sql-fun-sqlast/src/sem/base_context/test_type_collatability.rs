mod test_helpers;

use super::{BaseContext, BuiltinTypeInfo};
use crate::{
    sem::{FullName, TypeDefinition, TypeReference},
    test_helpers::TestParseContext,
};

#[rstest::rstest]
#[case::yes("true", Some(true))]
#[case::no("false", Some(false))]
#[case::undefined("null", None)]
fn required_nullable_fact_survives_loading_and_serialization(
    #[case] value: &str,
    #[case] expected: Option<bool>,
) {
    let json = format!(
        r#"{{"schema":"Odd.Schema","name":"_Type 雪","category":null,"collatable":{value}}}"#
    );
    let record: BuiltinTypeInfo = serde_json::from_str(&json).unwrap();
    let definition = TypeDefinition::from(&record);
    let reference = TypeReference::dynamic(&definition);
    let context = TestParseContext::default();
    assert_eq!(reference.collation_capability(&context), expected);
    assert_eq!(
        definition.full_name(),
        Some(&FullName::with_schema("Odd.Schema", "_Type 雪"))
    );
    let serialized = serde_json::to_value(&definition).unwrap();
    assert!(serialized["Builtin"].get("collatable").is_some());
    assert_eq!(
        serialized["Builtin"]["collatable"],
        serde_json::to_value(expected).unwrap()
    );
    let restored: TypeDefinition = serde_json::from_value(serialized).unwrap();
    assert_eq!(restored, definition);
    assert_eq!(
        TypeReference::dynamic(&restored).collation_capability(&context),
        expected
    );
}

#[rstest::rstest]
#[case::missing_schema(r#"{"name":"text","collatable":true}"#)]
#[case::null_schema(r#"{"schema":null,"name":"text","collatable":true}"#)]
#[case::missing_fact(r#"{"schema":"pg_catalog","name":"text"}"#)]
#[case::invalid_fact(r#"{"schema":"pg_catalog","name":"text","collatable":"true"}"#)]
fn invalid_required_fields_fail(#[case] json: &str) {
    assert!(serde_json::from_str::<BuiltinTypeInfo>(json).is_err());
}

#[test]
fn semantic_definitions_also_require_the_fact_key() {
    let definition = TypeDefinition::builtin(&FullName::with_schema("public", "custom"), None);
    let mut serialized = serde_json::to_value(definition).unwrap();
    serialized["Builtin"]
        .as_object_mut()
        .unwrap()
        .remove("collatable");
    assert!(serde_json::from_value::<TypeDefinition>(serialized).is_err());
}

#[rstest::rstest]
#[case::empty_schema(r#"[{"schema":"","name":"text","collatable":true}]"#)]
#[case::empty_name(r#"[{"schema":"public","name":"","collatable":true}]"#)]
#[case::nul_name(r#"[{"schema":"public","name":"a\u0000b","collatable":true}]"#)]
#[case::duplicate(r#"[{"schema":"public","name":"custom","collatable":true},{"schema":"public","name":"custom","collatable":false}]"#)]
fn invalid_catalog_reports_its_path(#[case] json: &str) {
    let fixture = test_helpers::TypeCatalog::new(json);
    let error = BaseContext::read_builtin_types(fixture.path()).unwrap_err();
    assert!(error.to_string().contains(fixture.path().to_str().unwrap()));
}

#[test]
fn same_local_names_retain_separate_qualified_facts() {
    let fixture = test_helpers::TypeCatalog::new(
        r#"[
        {"schema":"one","name":"custom","collatable":true},
        {"schema":"two","name":"custom","collatable":false}
    ]"#,
    );
    let types = BaseContext::read_builtin_types(fixture.path()).unwrap();
    let context = TestParseContext::default();
    for (schema, expected) in [("one", true), ("two", false)] {
        let name = FullName::with_schema(schema, "custom");
        let loaded = types.get(&name.to_string()).unwrap();
        assert_eq!(
            TypeReference::dynamic(loaded).collation_capability(&context),
            Some(expected)
        );
    }
}
