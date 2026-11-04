#[cfg(feature = "sqlite")]
use crate::ast::Dialect;
#[cfg(feature = "sqlite")]
use crate::context::FrozenBaseContext;
use crate::{
    context::{BaseParseContext, ConflictInfoRead, SchemaFileContext},
    names::{FullName, SchemaName},
    sem::{CatalogNameLookup, ParseContext, conflict_masters::lookup_collation},
    test_helpers::conflict_masters as test_helpers,
};

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
fn frozen_custom_context_preserves_completed_ambiguity() {
    let fixture = test_helpers::Masters::new();
    let mut context = fixture.context();
    let requested = FullName::from_local_name("custom");
    let candidates = [
        FullName::with_schema("one", "custom"),
        FullName::with_schema("two", "custom"),
    ];
    let outcome = CatalogNameLookup::ambiguous(&requested, &candidates);
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

#[cfg(feature = "sqlite")]
#[test]
#[should_panic(expected = "PostgreSQL master definitions must be supplied")]
fn unavailable_postgres_service_is_not_a_completed_missing_lookup() {
    let parent = test_helpers::base_without_masters(Dialect::sqlite()).unwrap();
    let args = parent.get_args().clone();
    let metadata = test_helpers::Masters::new().context().metadata().clone();
    let frozen = FrozenBaseContext::from(parent);
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
        .with_dialect(Dialect::sqlite());
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
