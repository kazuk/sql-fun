use crate::sem::{
    BaseParseContext, FullName, ParseContext, SchemaName, TypeDefinition, TypeReference,
};
use crate::test_helpers::TestParseContext;

#[rstest::rstest]
#[case::first_schema(&["a", "b", "pg_catalog"], None, Some("a"))]
#[case::reversed_path(&["b", "a", "pg_catalog"], None, Some("b"))]
#[case::qualified_outside_path(&["a"], Some("b"), Some("b"))]
#[case::empty_path(&[], None, Some("pg_catalog"))]
#[case::implicit_catalog_first(&["a", "b"], None, Some("pg_catalog"))]
#[case::explicit_catalog_last(&["a", "pg_catalog"], None, Some("a"))]
#[case::missing_schema(&["a"], Some("absent"), None)]
#[case::quoted_schema(&["a.b"], Some("a.b"), Some("a.b"))]
fn canonicalization_retains_the_matched_key(
    #[case] path: &[&str],
    #[case] qualifier: Option<&str>,
    #[case] expected_schema: Option<&str>,
    #[values(false, true)] array: bool,
) {
    let mut context = TestParseContext::default();
    let schemas: Vec<_> = path.iter().map(|name| SchemaName::from(*name)).collect();
    context.set_search_path(&schemas);
    let declaration = TypeDefinition::builtin(&FullName::from_local_name("item"));
    for schema in ["a", "b", "a.b", "pg_catalog"] {
        context.set_get_type_impl_result(&FullName::with_schema(schema, "item"), &declaration);
    }
    let name = match qualifier {
        Some(schema) => FullName::with_schema(schema, "item"),
        None => FullName::from_local_name("item"),
    };
    let expected = expected_schema.map(|schema| {
        TypeReference::concrete_type_ref(&FullName::with_schema(schema, "item"), array)
    });
    assert_eq!(
        context.canonicalize_type_ref(&TypeReference::concrete_type_ref(&name, array)),
        expected
    );
}
