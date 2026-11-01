//! FK identity is fixed at analysis time; dependency updates consume that binding.
use crate::{
    context::{ParseContext, TableInfoRead},
    sem::{ColumnName, FullName, PgBuiltInType},
    test_helpers::foreign_keys as fk,
};

#[rstest::rstest]
fn search_path_changes_do_not_rebind_existing_foreign_keys(
    #[values(
        fk::Declaration::inline(),
        fk::Declaration::create(),
        fk::Declaration::alter()
    )]
    declaration: fk::Declaration,
) {
    let mut context = declaration.bind(fk::parents(), "child", "parent(id)");
    assert!(context.problems().is_empty());
    assert_eq!(
        declaration.target(&context, "child"),
        Some(fk::name("a", "parent"))
    );
    fk::reversed_path(&mut context);
    let context = declaration.bind(context, "later", "parent(id)");
    assert_eq!(
        declaration.target(&context, "later"),
        Some(fk::name("b", "parent"))
    );
    let parent = context.freeze();
    let context = fk::apply(fk::child(&parent), "ALTER TABLE b.parent RENAME TO other");
    assert_eq!(
        declaration.target(&context, "child"),
        Some(fk::name("a", "parent"))
    );
    assert_eq!(
        declaration.target(&context, "later"),
        Some(fk::name("b", "other"))
    );
    let context = fk::apply(context, "ALTER TABLE a.parent RENAME TO renamed");
    assert_eq!(
        declaration.target(&context, "child"),
        Some(fk::name("a", "renamed"))
    );
    assert_eq!(
        declaration.target(&parent, "child"),
        Some(fk::name("a", "parent"))
    );
    assert_eq!(
        declaration.target(&parent, "later"),
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
    #[values(
        fk::Declaration::inline(),
        fk::Declaration::create(),
        fk::Declaration::alter()
    )]
    declaration: fk::Declaration,
) {
    let mut context = fk::apply(
        fk::parents(),
        r#"CREATE TABLE a."Parent"(id integer PRIMARY KEY)"#,
    );
    fk::reversed_path(&mut context);
    let context = declaration.bind(context, "child", reference);
    assert!(context.problems().is_empty());
    assert_eq!(
        declaration.target(&context, "child"),
        Some(fk::name(schema, local))
    );
}

#[rstest::rstest]
fn binding_uses_the_catalog_key_not_the_stored_declaration(
    #[values(
        fk::Declaration::inline(),
        fk::Declaration::create(),
        fk::Declaration::alter()
    )]
    declaration: fk::Declaration,
) {
    let context = fk::apply(fk::context(), "CREATE TABLE parent(id integer PRIMARY KEY)");
    let definition = context
        .get_table_impl(&FullName::with_schema("a", "parent"))
        .unwrap();
    assert!(definition.name().full_name().schema().is_none());
    let context = declaration.bind(context, "child", "parent(id)");
    assert_eq!(
        declaration.target(&context, "child"),
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
    #[values(fk::Declaration::inline(), fk::Declaration::alter())] declaration: fk::Declaration,
) {
    let mut context = declaration.bind(fk::parents(), "child", reference);
    assert_eq!(
        declaration.target(&context, "child"),
        resolved.map(|name| fk::name("a", name))
    );
    assert_eq!(context.problems().len(), 1);
    let expected_code = if declaration.is_table() {
        table_code
    } else {
        inline_code
    };
    assert_eq!(context.problems()[0].code(), expected_code);
    fk::reversed_path(&mut context);
    let context = fk::apply(context, "CREATE TABLE b.missing(id integer PRIMARY KEY)");
    let context = fk::apply(context, "ALTER TABLE b.missing RENAME TO appeared");
    assert_eq!(
        declaration.target(&context, "child"),
        resolved.map(|name| fk::name("a", name))
    );
    let context = fk::apply(context, "SELECT id FROM a.child");
    assert_eq!(context.problems().len(), 1);
    let parent = context.freeze();
    let context = fk::child(&parent);
    assert!(context.problems().is_empty());
    assert_eq!(
        declaration.target(&context, "child"),
        resolved.map(|name| fk::name("a", name))
    );
}

#[rstest::rstest]
fn relation_shadowing_does_not_bind_a_table_in_a_later_schema(
    #[values(fk::Declaration::inline(), fk::Declaration::alter())] declaration: fk::Declaration,
) {
    let context = fk::apply(
        fk::context(),
        "CREATE TABLE b.parent(id integer PRIMARY KEY)",
    );
    let context = fk::apply(context, "CREATE VIEW a.parent AS SELECT 1 AS id");
    let context = declaration.bind(context, "child", "parent(id)");
    assert!(declaration.target(&context, "child").is_none());
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

#[rstest::rstest]
#[case::create(fk::Declaration::create())]
#[case::alter(fk::Declaration::alter())]
fn table_foreign_key_attributes_survive_catalog_reads(#[case] declaration: fk::Declaration) {
    use crate::sem::{Deferrability, Validity};
    let context = declaration.bind(
        fk::parents(),
        "child",
        "parent(id) MATCH FULL ON DELETE CASCADE ON UPDATE SET NULL DEFERRABLE INITIALLY DEFERRED",
    );
    let table = context
        .get_table_impl(fk::name("a", "child").full_name())
        .unwrap();
    let constraint = table.constraint("fk").unwrap().as_foreign_key().unwrap();
    assert_eq!(constraint.source_columns(), &[ColumnName::from("id")]);
    assert_eq!(
        constraint.target_columns(),
        Some([ColumnName::from("id")].as_slice())
    );
    assert_eq!(
        ::serde_json::to_value(constraint.match_type()).unwrap(),
        "Full"
    );
    assert_eq!(
        ::serde_json::to_value(constraint.delete_action()).unwrap(),
        "Cascade"
    );
    assert_eq!(
        ::serde_json::to_value(constraint.update_action()).unwrap(),
        "SetNull"
    );
    assert_eq!(
        constraint.deferrability(),
        Some(&Deferrability::new(true, true))
    );
    assert_eq!(constraint.validity(), Some(&Validity::new(false, true)));
    assert!(context.problems().is_empty(), "{:?}", context.problems());
}

#[test]
fn create_result_and_catalog_retain_composite_and_unnamed_foreign_keys() -> testresult::TestResult {
    let context = fk::apply(
        fk::context(),
        "CREATE TABLE a.parent(x integer, y integer, PRIMARY KEY(x,y), UNIQUE(y,x))",
    );
    let pair = crate::test_helpers::ownership::apply_ownership_sql(
        context,
        "CREATE TABLE a.child(x integer, y integer, CONSTRAINT fk FOREIGN KEY(y,x) REFERENCES parent(y,x), FOREIGN KEY(x,y) REFERENCES parent, FOREIGN KEY(y,x) REFERENCES parent(y,x))",
    )?;
    let result = pair.ast().as_create_table().unwrap();
    assert_eq!(result.foreign_keys().count(), 3);
    let constraint = result.constraint("fk").unwrap().as_foreign_key().unwrap();
    assert_eq!(
        constraint.source_columns(),
        &[ColumnName::from("y"), ColumnName::from("x")]
    );
    assert_eq!(
        constraint.target_columns(),
        Some([ColumnName::from("y"), ColumnName::from("x")].as_slice())
    );
    let context = pair.into_context();
    let child = context
        .get_table_impl(fk::name("a", "child").full_name())
        .unwrap();
    assert_eq!(child.foreign_keys().count(), 3);
    assert_eq!(
        child
            .foreign_keys()
            .filter(|key| key.name().is_empty())
            .count(),
        2
    );
    assert!(child.foreign_keys().any(|key| key.target_columns()
        == Some([ColumnName::from("x"), ColumnName::from("y")].as_slice())));
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(())
}

#[rstest::rstest]
#[case::explicit("a.child(id)")]
#[case::unqualified("child(id)")]
#[case::omitted_columns("child")]
fn create_table_self_reference_uses_in_progress_keys(#[case] reference: &str) {
    let context = fk::apply(
        fk::context(),
        &format!(
            "CREATE TABLE a.child(id integer, parent_id integer, CONSTRAINT fk FOREIGN KEY(parent_id) REFERENCES {reference}, PRIMARY KEY(id))"
        ),
    );
    let child = context
        .get_table_impl(fk::name("a", "child").full_name())
        .unwrap();
    let constraint = child.constraint("fk").unwrap().as_foreign_key().unwrap();
    assert_eq!(constraint.target_table(), Some(&fk::name("a", "child")));
    assert_eq!(
        constraint.target_columns(),
        Some([ColumnName::from("id")].as_slice())
    );
    assert!(context.problems().is_empty(), "{:?}", context.problems());
}

#[test]
fn create_foreign_key_column_renames_update_owned_and_incoming_columns() {
    let context = fk::Declaration::create().bind(fk::parents(), "child", "parent(id)");
    let parent = context.freeze();
    let context = fk::apply(
        fk::child(&parent),
        "ALTER TABLE a.parent RENAME COLUMN id TO renamed",
    );
    let context = fk::apply(context, "ALTER TABLE a.child RENAME COLUMN id TO parent_id");
    let child = context
        .get_table_impl(fk::name("a", "child").full_name())
        .unwrap();
    let constraint = child.constraint("fk").unwrap().as_foreign_key().unwrap();
    assert_eq!(
        constraint.source_columns(),
        &[ColumnName::from("parent_id")]
    );
    assert_eq!(
        constraint.target_columns(),
        Some([ColumnName::from("renamed")].as_slice())
    );
    let original = parent
        .get_table_impl(fk::name("a", "child").full_name())
        .unwrap();
    assert_eq!(
        original
            .constraint("fk")
            .unwrap()
            .as_foreign_key()
            .unwrap()
            .source_columns(),
        &[ColumnName::from("id")]
    );
    let context = fk::apply(context, "SELECT parent_id FROM a.child");
    assert!(context.problems().is_empty(), "{:?}", context.problems());
}

#[rstest::rstest]
#[case::source("absent", "parent(id)", "column_not_found_in_table", true)]
#[case::postgres_case_sensitive("\"ID\"", "parent(id)", "column_not_found_in_table", true)]
#[case::target_column("id", "parent(absent)", "column_not_found_in_table", true)]
#[case::target_relation("id", "missing(id)", "relation_not_found", false)]
fn rejected_create_retains_analysis_without_publishing_table(
    #[case] source: &str,
    #[case] reference: &str,
    #[case] code: &str,
    #[case] bound: bool,
) -> testresult::TestResult {
    let context = fk::parents();
    let pair = crate::test_helpers::ownership::apply_ownership_sql(
        context,
        &format!(
            "CREATE TABLE a.child(id integer, CONSTRAINT fk FOREIGN KEY({source}) REFERENCES {reference})"
        ),
    )?;
    let table = pair.ast().as_create_table().unwrap();
    let constraint = table.constraint("fk").unwrap().as_foreign_key().unwrap();
    assert!(!constraint.references_can_apply());
    assert_eq!(
        constraint.target_table(),
        if bound {
            Some(fk::name("a", "parent"))
        } else {
            None
        }
        .as_ref()
    );
    let context = pair.into_context();
    assert!(
        context
            .get_table_impl(fk::name("a", "child").full_name())
            .is_none()
    );
    assert_eq!(context.problems().len(), 1);
    assert_eq!(context.problems()[0].code(), code);
    let context = fk::apply(context, "SELECT id FROM a.parent");
    assert_eq!(context.problems().len(), 1);
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::existing("Parent(id)", true)]
#[case::forward("later(id)", false)]
fn sqlite_create_table_foreign_keys_retain_declarations(
    #[case] reference: &str,
    #[case] resolved: bool,
) -> testresult::TestResult {
    let context = fk::apply_sqlite(fk::sqlite_context(), "CREATE TABLE Parent(id PRIMARY KEY)");
    let sql = format!(
        "CREATE TABLE child(id, CONSTRAINT fk FOREIGN KEY(id) REFERENCES {reference} ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED)"
    );
    let context = fk::apply_sqlite(context, &sql);
    let child = context
        .get_table_impl(fk::name("main", "child").full_name())
        .unwrap();
    let constraint = child.constraint("fk").unwrap().as_foreign_key().unwrap();
    assert_eq!(
        constraint.target_table(),
        if resolved {
            Some(fk::name("main", "parent"))
        } else {
            None
        }
        .as_ref()
    );
    assert_eq!(
        ::serde_json::to_value(constraint.delete_action())?,
        "Cascade"
    );
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch("CREATE TABLE Parent(id PRIMARY KEY)")?;
    oracle.execute_batch(&sql)?;
    let target: String = oracle.query_row(
        r#"SELECT "table" FROM pragma_foreign_key_list('child')"#,
        [],
        |row| row.get(0),
    )?;
    assert_eq!(target, if resolved { "Parent" } else { "later" });
    Ok(())
}

#[test]
fn delete_column_subset_survives_create_and_rename() {
    let context = fk::apply(
        fk::parents(),
        "CREATE TABLE a.child(id integer, extra integer, CONSTRAINT fk FOREIGN KEY(id) REFERENCES parent(id) ON DELETE SET NULL (id))",
    );
    let context = fk::apply(context, "ALTER TABLE a.child RENAME COLUMN id TO parent_id");
    let child = context
        .get_table_impl(fk::name("a", "child").full_name())
        .unwrap();
    let constraint = child.constraint("fk").unwrap().as_foreign_key().unwrap();
    assert_eq!(
        constraint.delete_columns(),
        &[ColumnName::from("parent_id")]
    );
    assert!(context.problems().is_empty(), "{:?}", context.problems());
}

#[test]
fn in_progress_self_reference_respects_an_earlier_search_path_relation() {
    let context = fk::apply(
        fk::context(),
        "CREATE TABLE a.child(id integer PRIMARY KEY)",
    );
    let context = fk::apply(
        context,
        "CREATE TABLE b.child(id integer PRIMARY KEY, parent_id integer, CONSTRAINT fk FOREIGN KEY(parent_id) REFERENCES child(id))",
    );
    let child = context
        .get_table_impl(fk::name("b", "child").full_name())
        .unwrap();
    assert_eq!(
        child
            .constraint("fk")
            .unwrap()
            .as_foreign_key()
            .unwrap()
            .target_table(),
        Some(&fk::name("a", "child"))
    );
    assert!(context.problems().is_empty(), "{:?}", context.problems());
}

#[test]
fn omitted_reference_columns_require_a_postgres_primary_key() -> testresult::TestResult {
    let context = fk::apply(fk::context(), "CREATE TABLE a.parent(id integer)");
    let pair = crate::test_helpers::ownership::apply_ownership_sql(
        context,
        "CREATE TABLE a.child(parent_id integer, FOREIGN KEY(parent_id) REFERENCES parent)",
    )?;
    let context = pair.into_context();
    assert_eq!(
        context.problems().len(),
        1,
        "missing primary key must produce a structured diagnostic"
    );
    assert_eq!(
        context.problems()[0].code(),
        "foreign_key_referenced_primary_key_missing"
    );
    assert!(
        context
            .get_table_impl(fk::name("a", "child").full_name())
            .is_none()
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::main("main", "temp")]
#[case::temporary("temp", "main")]
fn sqlite_foreign_key_uses_its_own_database(
    #[case] own: &str,
    #[case] other: &str,
) -> testresult::TestResult {
    let context = fk::apply_sqlite(
        fk::sqlite_context(),
        "CREATE TABLE main.parent(id PRIMARY KEY)",
    );
    let context = fk::apply_sqlite(context, "CREATE TEMP TABLE parent(id PRIMARY KEY)");
    let sql = format!(
        "CREATE TABLE {own}.child(id, CONSTRAINT fk FOREIGN KEY(id) REFERENCES parent(id))"
    );
    let context = fk::apply_sqlite(context, &sql);
    let child_name = fk::name(own, "child");
    let child = context.get_table_impl(child_name.full_name()).unwrap();
    assert_eq!(
        child
            .constraint("fk")
            .unwrap()
            .as_foreign_key()
            .unwrap()
            .target_table(),
        Some(&fk::name(own, "parent"))
    );
    let rename = format!("ALTER TABLE {other}.parent RENAME TO unrelated");
    let context = fk::apply_sqlite(context, &rename);
    let child = context.get_table_impl(child_name.full_name()).unwrap();
    assert_eq!(
        child
            .constraint("fk")
            .unwrap()
            .as_foreign_key()
            .unwrap()
            .target_table(),
        Some(&fk::name(own, "parent"))
    );
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch(
        "CREATE TABLE main.parent(id PRIMARY KEY); CREATE TEMP TABLE parent(id PRIMARY KEY)",
    )?;
    oracle.execute_batch(&sql)?;
    oracle.execute_batch(&rename)?;
    let target: String = oracle.query_row(
        r#"SELECT "table" FROM pragma_foreign_key_list('child', ?1)"#,
        [own],
        |row| row.get(0),
    )?;
    assert_eq!(target, "parent");
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_foreign_key_source_columns_ignore_ascii_case() -> testresult::TestResult {
    let context = fk::apply_sqlite(fk::sqlite_context(), "CREATE TABLE parent(id PRIMARY KEY)");
    let sql = r#"CREATE TABLE child("ID", CONSTRAINT fk FOREIGN KEY("id") REFERENCES parent(id))"#;
    let context = fk::apply_sqlite(context, sql);
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let child = context
        .get_table_impl(fk::name("main", "child").full_name())
        .unwrap();
    assert!(
        child
            .constraint("fk")
            .unwrap()
            .as_foreign_key()
            .unwrap()
            .references_can_apply()
    );
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch("CREATE TABLE parent(id PRIMARY KEY)")?;
    oracle.execute_batch(sql)?;
    Ok(())
}

#[rstest::rstest]
#[case::unnamed_inline("id integer PRIMARY KEY", vec![ColumnName::from("id")])]
#[case::named_inline("id integer CONSTRAINT pk PRIMARY KEY", vec![ColumnName::from("id")])]
#[case::unnamed_table("id integer, PRIMARY KEY(id)", vec![ColumnName::from("id")])]
#[case::named_table("id integer, CONSTRAINT pk PRIMARY KEY(id)", vec![ColumnName::from("id")])]
#[case::composite("id integer, other integer, PRIMARY KEY(other, id)", vec![ColumnName::from("other"), ColumnName::from("id")])]
#[case::absent("id integer", Vec::<ColumnName>::new())]
fn primary_key_columns_cover_inline_and_table_declarations(
    #[case] definition: &str,
    #[case] expected: Vec<ColumnName>,
) {
    let context = fk::apply(
        fk::context(),
        &format!("CREATE TABLE a.parent({definition})"),
    );
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let table = context
        .get_table_impl(fk::name("a", "parent").full_name())
        .unwrap();
    let actual: Vec<_> = table.primary_key_columns().into_iter().cloned().collect();
    assert_eq!(actual, expected);
}
