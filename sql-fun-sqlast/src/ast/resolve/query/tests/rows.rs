//! Row structure and field bindings use the small query catalog, without builtin loading.
use crate::{
    ast::{Dialect, resolve::query::test_helpers::resolve},
    context::ParseContext,
    names::ColumnName,
    sem::{PgBuiltInType, SemScalarExpr},
};

#[test]
fn row_fields_preserve_order_types_nullability_and_nested_shape() {
    let (query, context) = resolve(
        "SELECT ROW(1, NULL, ROW(true, 2::bigint)) AS value",
        Dialect::postgres(),
        true,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let value = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    let row = value.as_row().unwrap();
    assert_eq!(row.fields().len(), 3);
    assert_eq!(row.fields()[0].get_type(), Some(PgBuiltInType::int4()));
    assert_eq!(row.fields()[1].is_not_null(), Some(false));
    let nested = row.fields()[2].as_row().unwrap();
    assert_eq!(nested.fields()[0].get_type(), Some(PgBuiltInType::bool()));
    assert_eq!(nested.fields()[1].get_type(), Some(PgBuiltInType::int8()));
    assert!(value.get_type().unwrap().is_dynamic());
    let columns = value.composite_columns(&context).unwrap();
    assert_eq!(
        columns.column_at(0).unwrap().name(),
        &Some(ColumnName::from("f1"))
    );
    assert_eq!(columns.column_at(1).unwrap().is_not_null(), Some(false));
    let encoded = serde_json::to_string(value).unwrap();
    let decoded: SemScalarExpr = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value, &decoded);
}

#[rstest::rstest]
#[case::later("SELECT (ROW($1, 2)).f1 AS value, $1::integer AS hint")]
#[case::earlier("SELECT $1::integer AS hint, (ROW($1, 2)).f1 AS value")]
#[case::record_cast("SELECT (ROW($1,2)::record).f1 AS value, $1::integer AS hint")]
#[case::nested("SELECT ((ROW(2, ROW($1))).f2).f1 AS value, $1::integer AS hint")]
fn parameter_field_remains_connected_to_its_source(#[case] sql: &str) {
    let (query, context) = resolve(sql, Dialect::postgres(), true).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert_eq!(
        query
            .get_result_column_def(&ColumnName::from("value"))
            .unwrap()
            .get_type_cloned(),
        Some(PgBuiltInType::int4())
    );
    let expression = query
        .result_columns()
        .get_column_with_skip_count(&ColumnName::from("value"), 0)
        .unwrap()
        .expression()
        .unwrap();
    assert_eq!(expression.get_column_name(), "f1");
    assert!(expression.as_indirection().is_some());
    assert!(
        context
            .parameter_constraints()
            .allows(&std::collections::BTreeMap::from([(
                1,
                PgBuiltInType::int4()
            )]))
    );
    assert!(
        !context
            .parameter_constraints()
            .allows(&std::collections::BTreeMap::from([(
                1,
                PgBuiltInType::bool()
            )]))
    );
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::scalar("SELECT (1)", false)]
#[case::row("SELECT (1,2)", true)]
fn sqlite_projection_keeps_scalar_grouping_and_rejects_rows(
    #[case] sql: &str,
    #[case] invalid: bool,
) {
    let oracle = rusqlite::Connection::open_in_memory().unwrap();
    assert_eq!(oracle.prepare(sql).is_err(), invalid);
    let (query, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
    let value = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    assert_eq!(value.as_row().is_some(), invalid);
    assert_eq!(context.reported_problem_count() > 0, invalid);
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::nested_row("SELECT ((1,2),3) IN (SELECT 1,3)")]
#[case::cast("SELECT CAST((1,2) AS TEXT)")]
#[case::nested_cast("SELECT CAST(CAST((1,2) AS TEXT) AS TEXT)")]
#[case::unary_plus("SELECT +(1,2)")]
#[case::null_test("SELECT (1,2) IS NULL")]
#[case::not("SELECT NOT (1,2)")]
#[case::and("SELECT (1,2) AND 1")]
#[case::truth_test("SELECT (1,2) IS TRUE")]
#[case::collate("SELECT (1,2) COLLATE binary")]
#[case::parameter("SELECT CAST((?1,2) AS TEXT)")]
#[case::coalesce("SELECT coalesce((1,2), 3)")]
#[case::case_result("SELECT CASE WHEN 1 THEN (1,2) ELSE 3 END")]
#[case::case_condition("SELECT CASE WHEN (1,2) THEN 1 ELSE 3 END")]
#[case::case_argument("SELECT CASE (1,2) WHEN 1 THEN 2 END")]
#[case::case_default("SELECT CASE WHEN 1 THEN 2 ELSE (1,2) END")]
fn sqlite_scalar_consumers_reject_row_operands(#[case] sql: &str) {
    let oracle = rusqlite::Connection::open_in_memory().unwrap();
    assert!(oracle.prepare(sql).is_err(), "{sql}");
    let (_, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
    assert!(
        context
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "query_scalar_value_required"),
        "{sql}: {:?}",
        context.reported_problems()
    );
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::membership("SELECT (1,2) IN (SELECT 1,2)")]
#[case::is("SELECT (1,2) IS (1,2)")]
fn sqlite_row_consumers_still_accept_row_operands(#[case] sql: &str) {
    let oracle = rusqlite::Connection::open_in_memory().unwrap();
    assert_eq!(
        oracle
            .query_row(sql, [], |row| row.get::<_, i32>(0))
            .unwrap(),
        1
    );
    let (_, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
}

#[test]
fn every_row_field_retains_its_completed_binding() {
    let (query, context) = resolve(
        "SELECT ROW(t.x, public.absent.x, t.x) FROM (SELECT 1 AS x) t",
        Dialect::postgres(),
        false,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 1);
    let row = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_row()
        .unwrap();
    assert_eq!(
        row.fields()[0].constraint_binding(),
        Some(("t", &ColumnName::from("x")))
    );
    assert_eq!(
        row.fields()[1].qualification_failure().unwrap().code(),
        "column_source_not_found"
    );
    assert_eq!(
        row.fields()[2].constraint_binding(),
        Some(("t", &ColumnName::from("x")))
    );
}
