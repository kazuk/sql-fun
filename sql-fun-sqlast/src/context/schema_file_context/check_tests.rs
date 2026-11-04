//! CHECK binding, catalog transitions and dependency propagation through public analysis.
use crate::{
    ast::Dialect,
    context::{ParseContext, TableInfoRead},
    names::{ColumnName, FullName, TableName},
    sem::{AlterObjSubCommand, AnalyzeConstraint, Constraint, PgBuiltInType},
    test_helpers::{checks, foreign_keys, statements},
};

#[test]
fn create_retains_all_checks_and_late_column_bindings() -> testresult::TestResult {
    let pair = checks::apply(
        checks::context(Dialect::postgres()),
        "CREATE TABLE public.bounds(a integer, CHECK(a < b), b integer, CONSTRAINT named CHECK(bounds.b IS NULL), CHECK(NULL), CHECK('yes'))",
    )?;
    let table = pair.ast().as_create_table().unwrap();
    assert_eq!(table.checks().count(), 4);
    assert!(table.checks().all(|check| check.can_apply()));
    let check = table.constraint("named").unwrap().as_check().unwrap();
    assert_eq!(check.expression().get_type(), Some(PgBuiltInType::bool()));
    let encoded = ::serde_json::to_string(check.expression())?;
    assert!(encoded.contains("TableColumn"), "{encoded}");
    assert!(encoded.contains("int4"), "{encoded}");
    assert!(
        table
            .constraint("named")
            .unwrap()
            .implicit_changes()?
            .is_empty()
    );
    assert_ne!(
        table
            .get_column_def(&ColumnName::from("a"))
            .unwrap()
            .is_not_null(),
        Some(true)
    );
    let context = pair.into_context();
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    assert_eq!(
        context
            .get_table_impl(&FullName::with_schema("public", "bounds"))
            .unwrap()
            .checks()
            .count(),
        4
    );
    Ok(())
}

#[rstest::rstest]
#[case::integer("a", "check_boolean_type")]
#[case::nested_integer("a AND true", "check_boolean_type")]
#[case::invalid_case("CASE WHEN a THEN true ELSE false END", "check_boolean_type")]
#[case::invalid_literal("'bad'", "check_boolean_literal")]
#[case::missing("missing IS NULL", "column_not_found_in_table")]
#[case::qualified_same_name("bounds.bounds IS NULL", "column_not_found_in_table")]
#[case::schema_qualified_same_name("public.bounds.bounds IS NULL", "column_not_found_in_table")]
#[case::wrong_table("absent.a IS NULL", "undefined_alias")]
#[case::parameter("$1", "check_parameter")]
#[case::subquery("(SELECT true)", "check_expression_placement")]
fn rejected_create_retains_expression_and_does_not_publish(
    #[case] expression: &str,
    #[case] diagnostic: &str,
) -> testresult::TestResult {
    let pair = checks::apply(
        checks::context(Dialect::postgres()),
        &format!("CREATE TABLE public.bounds(a integer, CHECK({expression}))"),
    )?;
    let table = pair.ast().as_create_table().unwrap();
    assert!(!table.checks().next().unwrap().can_apply());
    let context = pair.into_context();
    assert!(
        context
            .get_table_impl(&FullName::with_schema("public", "bounds"))
            .is_none()
    );
    assert!(
        context
            .problems()
            .iter()
            .any(|problem| problem.code() == diagnostic),
        "{:?}",
        context.problems()
    );
    let pair = checks::apply(context, "CREATE TABLE public.later(a integer, CHECK(true))")?;
    assert!(
        pair.context()
            .get_table_impl(&FullName::with_schema("public", "later"))
            .is_some()
    );
    Ok(())
}

#[test]
fn alter_retains_options_unnamed_checks_and_atomic_rejection() -> testresult::TestResult {
    let pair = checks::apply(
        checks::context(Dialect::postgres()),
        "CREATE TABLE public.bounds(a integer, b integer)",
    )?;
    let pair = checks::apply(
        pair.into_context(),
        "ALTER TABLE public.bounds ADD CONSTRAINT ordered CHECK(a < b) NO INHERIT NOT VALID, ADD CHECK(NULL), ADD CHECK(true)",
    )?;
    let table = pair
        .context()
        .get_table_impl(&FullName::with_schema("public", "bounds"))
        .unwrap();
    assert_eq!(table.checks().count(), 3);
    let check = table.constraint("ordered").unwrap().as_check().unwrap();
    assert!(check.no_inherit());
    assert!(check.not_valid());
    assert!(!check.validated());
    assert_eq!(check.enforced(), None);
    let pair = checks::apply(
        pair.into_context(),
        "ALTER TABLE public.bounds ADD CONSTRAINT valid CHECK(a IS NULL), ADD CONSTRAINT invalid CHECK(a)",
    )?;
    assert!(!pair.ast().alter_table_statement().unwrap().can_apply());
    let table = pair
        .context()
        .get_table_impl(&FullName::with_schema("public", "bounds"))
        .unwrap();
    assert_eq!(table.checks().count(), 3);
    assert!(table.constraint("valid").is_none());
    Ok(())
}

#[test]
fn duplicate_check_names_retain_rejected_semantics() -> testresult::TestResult {
    let pair = checks::apply(
        checks::context(Dialect::postgres()),
        "CREATE TABLE public.bounds(a integer, CONSTRAINT repeated CHECK(true), CONSTRAINT repeated CHECK(false))",
    )?;
    let table = pair.ast().as_create_table().unwrap();
    assert_eq!(table.checks().count(), 2);
    assert_eq!(table.checks().filter(|check| !check.can_apply()).count(), 1);
    assert!(
        pair.context()
            .get_table_impl(&FullName::with_schema("public", "bounds"))
            .is_none()
    );
    Ok(())
}

#[test]
fn check_bindings_follow_renames_without_changing_frozen_layers() -> testresult::TestResult {
    let pair = checks::apply(
        checks::context(Dialect::postgres()),
        "CREATE TABLE bounds(a integer, b integer, CONSTRAINT ordered CHECK(a < b))",
    )?;
    let frozen = pair.into_context().freeze();
    let context = statements::layer(&frozen, std::path::Path::new("renames.sql"));
    let pair = checks::apply(
        context,
        "ALTER TABLE public.bounds RENAME COLUMN a TO first",
    )?;
    let pair = checks::apply(
        pair.into_context(),
        "ALTER TABLE public.bounds RENAME TO renamed",
    )?;
    let table = pair
        .context()
        .get_table_impl(&FullName::with_schema("public", "renamed"))
        .unwrap();
    let check = table.constraint("ordered").unwrap().as_check().unwrap();
    assert_eq!(
        check.table().unwrap().full_name(),
        &FullName::with_schema("public", "renamed")
    );
    let encoded = ::serde_json::to_string(check.expression())?;
    assert!(encoded.contains("first"), "{encoded}");
    assert!(encoded.contains("renamed"), "{encoded}");
    let original = frozen
        .get_table_impl(&FullName::with_schema("public", "bounds"))
        .unwrap();
    assert!(
        ::serde_json::to_string(
            original
                .constraint("ordered")
                .unwrap()
                .as_check()
                .unwrap()
                .expression()
        )?
        .contains("bounds")
    );
    assert!(original.get_column_def(&ColumnName::from("a")).is_some());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::numeric("a")]
#[case::text("'abc'")]
#[case::cross_column("a IS NULL OR b IS NOT NULL")]
fn sqlite_truth_values_are_not_postgres_boolean_coercions(
    #[case] expression: &str,
) -> testresult::TestResult {
    let sql = format!("CREATE TABLE bounds(a integer, b integer, CHECK({expression}))");
    let pair = checks::apply(checks::context(Dialect::sqlite()), &sql)?;
    let table = pair.ast().as_create_table().unwrap();
    assert!(table.checks().next().unwrap().can_apply());
    assert!(
        pair.context().problems().is_empty(),
        "{:?}",
        pair.context().problems()
    );
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch(&sql)?;
    Ok(())
}

#[test]
fn legacy_constraint_entry_point_retains_completed_bindings() -> testresult::TestResult {
    let pair = checks::apply(
        checks::context(Dialect::postgres()),
        "CREATE TABLE bounds(a integer, b integer)",
    )?;
    let mut context = pair.into_context();
    let syntax = foreign_keys::postgres_constraint(
        "ALTER TABLE bounds ADD CONSTRAINT ordered CHECK(a < b) NO INHERIT NOT VALID",
    );
    let analyzed = Constraint::analyze(
        &mut context,
        Some(&TableName::from(FullName::from_local_name("bounds"))),
        syntax,
    )?;
    let check = analyzed.as_check().unwrap();
    assert!(check.can_apply());
    assert!(check.no_inherit());
    assert!(check.not_valid());
    assert!(!check.validated());
    assert_eq!(
        check.table().unwrap().full_name(),
        &FullName::with_schema("public", "bounds")
    );
    assert_eq!(check.expression().get_type(), Some(PgBuiltInType::bool()));
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(())
}

#[test]
fn create_not_valid_is_retained_but_effectively_validated() -> testresult::TestResult {
    let pair = checks::apply(
        checks::context(Dialect::postgres()),
        "CREATE TABLE bounds(a integer, CONSTRAINT positive CHECK(a < 10) NOT VALID)",
    )?;
    let check = pair
        .ast()
        .as_create_table()
        .unwrap()
        .constraint("positive")
        .unwrap()
        .as_check()
        .unwrap();
    assert!(check.can_apply());
    assert!(check.not_valid());
    assert!(check.validated());
    assert!(
        pair.context().problems().is_empty(),
        "{:?}",
        pair.context().problems()
    );
    Ok(())
}

#[test]
fn duplicate_names_within_one_alter_are_inapplicable_before_application() -> testresult::TestResult
{
    let pair = checks::apply(
        checks::context(Dialect::postgres()),
        "CREATE TABLE bounds(a integer)",
    )?;
    let pair = checks::apply(
        pair.into_context(),
        "ALTER TABLE bounds ADD CONSTRAINT repeated CHECK(true), ADD CONSTRAINT repeated CHECK(false)",
    )?;
    assert!(!pair.ast().alter_table_statement().unwrap().can_apply());
    assert_eq!(
        pair.context()
            .get_table_impl(&FullName::with_schema("public", "bounds"))
            .unwrap()
            .checks()
            .count(),
        0
    );
    assert!(
        pair.context()
            .problems()
            .iter()
            .any(|problem| problem.code() == "check_duplicate_name")
    );
    Ok(())
}

#[test]
fn missing_alter_target_is_a_completed_failed_binding() -> testresult::TestResult {
    let pair = checks::apply(
        checks::context(Dialect::postgres()),
        "ALTER TABLE missing ADD CHECK(true)",
    )?;
    let alter = pair.ast().alter_table_statement().unwrap();
    assert!(!alter.can_apply());
    let [AlterObjSubCommand::AddConstraint(constraint)] = alter.commands().as_slice() else {
        panic!("retained ADD CHECK")
    };
    let check = constraint.as_check().unwrap();
    assert!(check.table().is_none());
    assert!(!check.can_apply());
    assert!(
        pair.context()
            .get_table_impl(&FullName::with_schema("public", "missing"))
            .is_none()
    );
    Ok(())
}

#[rstest::rstest]
#[case::bare_row("a integer", "bounds IS NULL", "WholeRow")]
#[case::star_row("a integer", "bounds.* IS NULL", "WholeRow")]
#[case::shadowing_column("bounds integer", "bounds.bounds IS NULL", "ColumnRef")]
fn whole_row_lookup_preserves_column_qualification(
    #[case] columns: &str,
    #[case] expression: &str,
    #[case] binding: &str,
) -> testresult::TestResult {
    let pair = checks::apply(
        checks::context(Dialect::postgres()),
        &format!("CREATE TABLE bounds({columns}, CHECK({expression}))"),
    )?;
    let check = pair
        .ast()
        .as_create_table()
        .unwrap()
        .checks()
        .next()
        .unwrap();
    assert!(check.can_apply());
    let encoded = ::serde_json::to_string(check.expression())?;
    assert!(encoded.contains(binding), "{encoded}");
    assert!(
        pair.context().problems().is_empty(),
        "{:?}",
        pair.context().problems()
    );
    assert!(
        pair.context()
            .get_table_impl(&FullName::with_schema("public", "bounds"))
            .is_some()
    );
    Ok(())
}

#[rstest::rstest]
#[case::table_keys(
    "a integer, CONSTRAINT x UNIQUE(a), CONSTRAINT x UNIQUE(a), CONSTRAINT x CHECK(true)"
)]
#[case::column_keys(
    "a integer CONSTRAINT x UNIQUE, b integer CONSTRAINT x UNIQUE, CONSTRAINT x CHECK(true)"
)]
fn duplicate_keys_sharing_a_check_name_allow_later_statement_recovery(
    #[case] declarations: &str,
) -> testresult::TestResult {
    let pair = checks::apply(
        checks::context(Dialect::postgres()),
        &format!("CREATE TABLE bounds({declarations})"),
    )?;
    assert!(
        pair.context()
            .get_table_impl(&FullName::with_schema("public", "bounds"))
            .is_none()
    );
    assert!(
        pair.context()
            .problems()
            .iter()
            .any(|problem| problem.code() == "conflicting_creation")
    );
    let pair = checks::apply(
        pair.into_context(),
        "CREATE TABLE later(a integer, CHECK(true))",
    )?;
    assert!(
        pair.context()
            .get_table_impl(&FullName::with_schema("public", "later"))
            .is_some()
    );
    Ok(())
}
