//! FK identity is fixed at analysis time; dependency updates consume that binding.
use crate::{
    context::{ParseContext, TableInfoRead},
    sem::{ColumnName, FullName, PgBuiltInType},
    test_helpers::foreign_keys as fk,
};

#[rstest::rstest]
fn search_path_changes_do_not_rebind_existing_foreign_keys(
    #[values(false, true)] table_constraint: bool,
) {
    let mut context = fk::bind(fk::parents(), "child", "parent(id)", table_constraint);
    assert!(context.problems().is_empty());
    assert_eq!(
        fk::target(&context, "child", table_constraint),
        Some(fk::name("a", "parent"))
    );
    fk::reversed_path(&mut context);
    let context = fk::bind(context, "later", "parent(id)", table_constraint);
    assert_eq!(
        fk::target(&context, "later", table_constraint),
        Some(fk::name("b", "parent"))
    );
    let parent = context.freeze();
    let context = fk::apply(fk::child(&parent), "ALTER TABLE b.parent RENAME TO other");
    assert_eq!(
        fk::target(&context, "child", table_constraint),
        Some(fk::name("a", "parent"))
    );
    assert_eq!(
        fk::target(&context, "later", table_constraint),
        Some(fk::name("b", "other"))
    );
    let context = fk::apply(context, "ALTER TABLE a.parent RENAME TO renamed");
    assert_eq!(
        fk::target(&context, "child", table_constraint),
        Some(fk::name("a", "renamed"))
    );
    assert_eq!(
        fk::target(&parent, "child", table_constraint),
        Some(fk::name("a", "parent"))
    );
    assert_eq!(
        fk::target(&parent, "later", table_constraint),
        Some(fk::name("b", "parent"))
    );
    assert!(context.problems().is_empty());
    let context = fk::apply(context, "SELECT id FROM a.child");
    assert!(context.problems().is_empty());
}

#[rstest::rstest]
#[case::qualified("a.parent(id)", "a", "parent")]
#[case::quoted(r#"a."Parent"(id)"#, "a", "Parent")]
fn explicit_and_quoted_targets_remain_exact(
    #[case] reference: &str,
    #[case] schema: &str,
    #[case] local: &str,
    #[values(false, true)] table_constraint: bool,
) {
    let mut context = fk::apply(
        fk::parents(),
        r#"CREATE TABLE a."Parent"(id integer PRIMARY KEY)"#,
    );
    fk::reversed_path(&mut context);
    let context = fk::bind(context, "child", reference, table_constraint);
    assert!(context.problems().is_empty());
    assert_eq!(
        fk::target(&context, "child", table_constraint),
        Some(fk::name(schema, local))
    );
}

#[rstest::rstest]
fn binding_uses_the_catalog_key_not_the_stored_declaration(
    #[values(false, true)] table_constraint: bool,
) {
    let context = fk::apply(fk::context(), "CREATE TABLE parent(id integer PRIMARY KEY)");
    let definition = context
        .get_table_impl(&FullName::with_schema("a", "parent"))
        .unwrap();
    assert!(definition.name().full_name().schema().is_none());
    let context = fk::bind(context, "child", "parent(id)", table_constraint);
    assert_eq!(
        fk::target(&context, "child", table_constraint),
        Some(fk::name("a", "parent"))
    );
    assert!(context.problems().is_empty());
}

#[rstest::rstest]
#[case::missing_relation("missing(id)", None, "table_not_found", "relation_not_found")]
#[case::missing_column(
    "parent(absent)",
    Some("parent"),
    "column_not_found_in_table",
    "column_not_found_in_table"
)]
fn failed_lookup_is_retained_without_retroactive_resolution(
    #[case] reference: &str,
    #[case] resolved: Option<&str>,
    #[case] inline_code: &str,
    #[case] table_code: &str,
    #[values(false, true)] table_constraint: bool,
) {
    let mut context = fk::bind(fk::parents(), "child", reference, table_constraint);
    assert_eq!(
        fk::target(&context, "child", table_constraint),
        resolved.map(|name| fk::name("a", name))
    );
    assert_eq!(context.problems().len(), 1);
    let expected_code = if table_constraint {
        table_code
    } else {
        inline_code
    };
    assert_eq!(context.problems()[0].code(), expected_code);
    fk::reversed_path(&mut context);
    let context = fk::apply(context, "CREATE TABLE b.missing(id integer PRIMARY KEY)");
    let context = fk::apply(context, "ALTER TABLE b.missing RENAME TO appeared");
    assert_eq!(
        fk::target(&context, "child", table_constraint),
        resolved.map(|name| fk::name("a", name))
    );
    let context = fk::apply(context, "SELECT id FROM a.child");
    assert_eq!(context.problems().len(), 1);
    let parent = context.freeze();
    let context = fk::child(&parent);
    assert!(context.problems().is_empty());
    assert_eq!(
        fk::target(&context, "child", table_constraint),
        resolved.map(|name| fk::name("a", name))
    );
}

#[rstest::rstest]
fn relation_shadowing_does_not_bind_a_table_in_a_later_schema(
    #[values(false, true)] table_constraint: bool,
) {
    let context = fk::apply(
        fk::context(),
        "CREATE TABLE b.parent(id integer PRIMARY KEY)",
    );
    let context = fk::apply(context, "CREATE VIEW a.parent AS SELECT 1 AS id");
    let context = fk::bind(context, "child", "parent(id)", table_constraint);
    assert!(fk::target(&context, "child", table_constraint).is_none());
    assert_eq!(context.problems().len(), 1);
}

#[test]
fn inline_type_and_current_serialization_preserve_completed_outcomes() -> testresult::TestResult {
    let context = fk::bind(
        fk::parents(),
        "child",
        "parent(id) ON DELETE CASCADE NOT DEFERRABLE",
        false,
    );
    let constraint = fk::inline(&context, &FullName::with_schema("a", "child"));
    assert_eq!(
        constraint.primary_key_column_type(),
        Some(&PgBuiltInType::int4())
    );
    assert_eq!(constraint.primary_key_column(), &ColumnName::from("id"));
    let json = ::serde_json::to_value(&constraint)?;
    assert_eq!(json["delete_action"], "Cascade");
    assert_eq!(json["deferrablity"]["deferrable"], false);
    let restored: crate::sem::ForeignKeyColumnConstraint = ::serde_json::from_value(json)?;
    assert_eq!(restored, constraint);
    let context = fk::bind(context, "unresolved", "missing(id)", false);
    let unresolved = fk::inline(&context, &FullName::with_schema("a", "unresolved"));
    assert!(unresolved.primary_key_table().is_none());
    let json = ::serde_json::to_value(&unresolved)?;
    assert!(json["primary_key_table"].is_null());
    let restored: crate::sem::ForeignKeyColumnConstraint = ::serde_json::from_value(json)?;
    assert_eq!(restored, unresolved);
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_target_identity_follows_case_insensitive_table_rename() -> testresult::TestResult {
    let context = fk::apply_sqlite(fk::sqlite_context(), "CREATE TABLE Parent(id PRIMARY KEY)");
    let context = fk::apply_sqlite(context, "CREATE TABLE child(id REFERENCES PARENT(id))");
    let constraint = fk::inline(&context, &FullName::with_schema("main", "child"));
    assert_eq!(
        constraint.primary_key_table(),
        Some(&fk::name("main", "parent"))
    );
    let parent = context.freeze();
    let context = fk::apply_sqlite(fk::child(&parent), "ALTER TABLE pArEnT RENAME TO renamed");
    let constraint = fk::inline(&context, &FullName::with_schema("main", "child"));
    assert_eq!(
        constraint.primary_key_table(),
        Some(&fk::name("main", "renamed"))
    );
    assert!(context.problems().is_empty());
    assert_eq!(
        fk::inline(&parent, &FullName::with_schema("main", "child")).primary_key_table(),
        Some(&fk::name("main", "parent"))
    );
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch("CREATE TABLE Parent(id PRIMARY KEY); CREATE TABLE child(id REFERENCES PARENT(id)); ALTER TABLE pArEnT RENAME TO renamed;")?;
    let target: String = oracle.query_row(
        r#"SELECT "table" FROM pragma_foreign_key_list('child')"#,
        [],
        |row| row.get(0),
    )?;
    assert_eq!(target, "renamed");
    Ok(())
}
