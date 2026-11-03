use super::{lookup_collation, lookup_operator_class};
use crate::{
    context::{BaseParseContext, ConflictInfoRead, SchemaFileContext},
    names::{FullName, SchemaName},
    sem::ParseContext,
    test_helpers::conflict_masters as test_helpers,
};

#[rstest::rstest]
#[case::unqualified("C", None, Some("pg_catalog"))]
#[case::qualified("C", Some("pg_catalog"), Some("pg_catalog"))]
#[case::missing("absent", None, None)]
#[case::wrong_schema("C", Some("absent"), None)]
fn resolves_collation_names(
    #[case] name: &str,
    #[case] schema: Option<&str>,
    #[case] expected: Option<&str>,
) {
    let fixture = test_helpers::Masters::new();
    let context = fixture.context();
    let requested = if let Some(schema) = schema {
        FullName::with_schema(schema, name)
    } else {
        FullName::from_local_name(name)
    };
    let result = lookup_collation(&context, &requested);
    assert_eq!(result.requested(), &requested);
    assert_eq!(
        result.resolved_name(),
        expected
            .map(|schema| FullName::with_schema(schema, name))
            .as_ref()
    );
    assert_eq!(result.is_missing(), expected.is_none());
}

#[test]
fn ignores_ineligible_schema_before_shadowing() {
    let mut fixture = test_helpers::Masters::new();
    fixture.add_collation("first", "choice", 8, "c");
    fixture.add_collation("second", "choice", -1, "c");
    let mut context = fixture.context();
    context.set_search_path(&[SchemaName::from("first"), SchemaName::from("second")]);
    let result = lookup_collation(&context, &FullName::from_local_name("choice"));
    assert_eq!(
        result.resolved_name(),
        Some(&FullName::with_schema("second", "choice"))
    );
}

#[rstest::rstest]
#[case::implicit_catalog(false, "pg_catalog")]
#[case::explicit_catalog(true, "public")]
fn honors_catalog_position(#[case] explicit_catalog: bool, #[case] expected: &str) {
    let mut fixture = test_helpers::Masters::new();
    fixture.add_collation("public", "C", -1, "c");
    let mut context = fixture.context();
    let mut path = vec![SchemaName::from("public")];
    if explicit_catalog {
        path.push(SchemaName::from("pg_catalog"));
    }
    context.set_search_path(&path);
    let result = lookup_collation(&context, &FullName::from_local_name("C"));
    assert_eq!(
        result.resolved_name(),
        Some(&FullName::with_schema(expected, "C"))
    );
}

#[test]
fn child_lookup_uses_current_path_and_freeze_preserves_it() {
    let mut fixture = test_helpers::Masters::new();
    fixture.add_collation("first", "Mixed.名", -1, "c");
    fixture.add_collation("second", "Mixed.名", -1, "c");
    let mut parent = fixture.context();
    parent.set_search_path(&[SchemaName::from("first")]);
    let args = parent.get_args().clone();
    let metadata = parent.metadata().clone();
    let frozen = parent.freeze();
    let mut child = SchemaFileContext::new(
        std::path::Path::new("selected.sql"),
        &frozen,
        &args,
        &metadata,
    );
    child.set_search_path(&[SchemaName::from("second")]);
    let requested = FullName::from_local_name("Mixed.名");
    assert_eq!(
        child.lookup_collation(&requested).resolved_name(),
        Some(&FullName::with_schema("second", "Mixed.名"))
    );
    let child = child.freeze();
    assert_eq!(
        child.lookup_collation(&requested).resolved_name(),
        Some(&FullName::with_schema("second", "Mixed.名"))
    );
    assert_eq!(
        lookup_collation(&frozen, &requested).resolved_name(),
        Some(&FullName::with_schema("first", "Mixed.名"))
    );
    assert_eq!(child.database_collation(), frozen.database_collation());
    let qualified = FullName::with_schema("second", "Mixed.名");
    assert_eq!(
        child.collation_definition(&qualified),
        frozen.collation_definition(&qualified)
    );
    assert!(child.collation_definition(&requested).is_none());
    assert!(
        child
            .operator_class_definition(&FullName::from_local_name("ops"), "btree")
            .is_none()
    );
    assert!(
        child
            .operator_class_definition(&FullName::with_schema("public", "ops"), "gin")
            .is_none()
    );
}

#[test]
fn operator_class_reads_retain_method_family_type_and_default() {
    let fixture = test_helpers::Masters::new();
    let context = fixture.context();
    let name = FullName::with_schema("public", "ops");
    for method in ["btree", "hash"] {
        let result = lookup_operator_class(&context, &FullName::from_local_name("ops"), method);
        assert_eq!(result.resolved_name(), Some(&name));
        let definition = context.operator_class_definition(&name, method).unwrap();
        assert_eq!(definition.family().access_method(), method);
        assert_eq!(
            definition.input_type().qualified_name(),
            FullName::with_schema("outside", "a_type")
        );
        assert!(definition.default());
        assert_eq!(definition.key_type(), &None);
    }
    assert!(lookup_operator_class(&context, &name, "gin").is_missing());
}

#[test]
fn frozen_custom_context_preserves_completed_ambiguity() {
    let fixture = test_helpers::Masters::new();
    let mut context = fixture.context();
    let requested = FullName::from_local_name("custom");
    let candidates = [
        FullName::with_schema("one", "custom"),
        FullName::with_schema("two", "custom"),
    ];
    let outcome = crate::sem::CatalogNameLookup::ambiguous(&requested, &candidates);
    context.set_collation_lookup(&requested, &outcome);
    let frozen = context.freeze();
    assert_eq!(frozen.lookup_collation(&requested), outcome);
    assert_eq!(
        frozen
            .clone()
            .lookup_collation(&requested)
            .ambiguous_candidates(),
        Some(candidates.as_slice())
    );
}

#[test]
fn unqualified_lookup_skips_temporary_namespaces() {
    let mut fixture = test_helpers::Masters::new();
    fixture.add_collation("pg_temp_42", "temp_only", -1, "c");
    let mut context = fixture.context();
    context.set_search_path(&[SchemaName::from("pg_temp_42")]);
    assert!(lookup_collation(&context, &FullName::from_local_name("temp_only")).is_missing());
    assert!(
        lookup_collation(&context, &FullName::with_schema("pg_temp_42", "temp_only"))
            .resolved_name()
            .is_some()
    );
}

#[cfg(feature = "sqlite")]
#[test]
#[should_panic(expected = "PostgreSQL master definitions must be supplied")]
fn unavailable_postgres_service_is_not_a_completed_missing_lookup() {
    let parent = test_helpers::base_without_masters(crate::ast::Dialect::sqlite()).unwrap();
    let args = parent.get_args().clone();
    let metadata = test_helpers::Masters::new().context().metadata().clone();
    let frozen = crate::context::FrozenBaseContext::from(parent);
    let child = SchemaFileContext::new(
        std::path::Path::new("postgres.sql"),
        &frozen,
        &args,
        &metadata,
    );
    let _lookup = child.lookup_collation(&FullName::from_local_name("C"));
}

#[cfg(feature = "sqlite")]
#[test]
fn active_sqlite_dialect_does_not_use_frozen_postgres_namespace() {
    let fixture = test_helpers::Masters::new();
    let parent = fixture.context();
    let args = parent.get_args().clone();
    let sqlite = test_helpers::Masters::new()
        .context()
        .with_dialect(crate::ast::Dialect::sqlite());
    let frozen = parent.freeze();
    let child = SchemaFileContext::new(
        std::path::Path::new("sqlite.sql"),
        &frozen,
        &args,
        sqlite.metadata(),
    );
    assert!(
        child
            .lookup_collation(&FullName::from_local_name("binary"))
            .resolved_name()
            .is_some()
    );
    assert!(
        child
            .lookup_collation(&FullName::from_local_name("C"))
            .is_missing()
    );
    assert!(
        child
            .lookup_operator_class(&FullName::from_local_name("ops"), "btree")
            .is_missing()
    );
}
