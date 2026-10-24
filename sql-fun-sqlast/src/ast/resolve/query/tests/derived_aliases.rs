//! Derived-column visibility through the maintained SQL-to-semantics boundary.
use super::super::test_helpers::resolve;
use crate::ast::Dialect;
use crate::sem::{ColumnReferenceExpr, PgBuiltInType, SemScalarExpr};

#[rstest::rstest]
#[case::union(
    "SELECT renamed FROM (SELECT 1 AS n UNION ALL SELECT 2 AS n) x(renamed)",
    "SELECT x.renamed FROM (SELECT 1 AS n UNION ALL SELECT 2 AS n) x(renamed)"
)]
#[case::simple(
    "SELECT renamed FROM (SELECT 1 AS n) x(renamed)",
    "SELECT x.renamed FROM (SELECT 1 AS n) x(renamed)"
)]
#[case::partial(
    "SELECT b FROM (SELECT 1 AS a, 2 AS b) x(renamed)",
    "SELECT x.b FROM (SELECT 1 AS a, 2 AS b) x(renamed)"
)]
#[case::renamed_partial(
    "SELECT renamed FROM (SELECT 1 AS a, 2 AS b) x(renamed)",
    "SELECT x.renamed FROM (SELECT 1 AS a, 2 AS b) x(renamed)"
)]
#[case::hidden_name_does_not_compete(
    "SELECT n FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y",
    "SELECT y.n FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y"
)]
#[case::local_shadowing(
    "SELECT (SELECT renamed FROM (SELECT 2 AS n) y(renamed)) FROM (SELECT 1 AS n) x(renamed)",
    "SELECT (SELECT y.renamed FROM (SELECT 2 AS n) y(renamed)) FROM (SELECT 1 AS n) x(renamed)"
)]
#[case::correlated(
    "SELECT (SELECT renamed FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x(renamed)",
    "SELECT (SELECT x.renamed FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x(renamed)"
)]
#[case::hidden_local_name_allows_correlation(
    "SELECT (SELECT n FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x",
    "SELECT (SELECT x.n FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x"
)]
#[case::qualified_disambiguates(
    "SELECT x.renamed FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y(renamed)",
    "SELECT x.renamed FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y(renamed)"
)]
fn visible_columns_retain_qualified_binding(#[case] sql: &str, #[case] qualified: &str) {
    let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let (expected, context) = resolve(qualified, Dialect::postgres(), false).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let column = query.result_columns().get_column_at(0).unwrap();
    let expected_column = expected.result_columns().get_column_at(0).unwrap();
    // Equality includes the source alias, requested name and retained column definition.
    assert_eq!(column.expression(), expected_column.expression());
    assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
    assert_eq!(
        column.expression().unwrap().is_not_null(),
        expected_column.expression().unwrap().is_not_null()
    );
    assert_eq!(column.column_name(), expected_column.column_name());
}

#[rstest::rstest]
#[case::simple("SELECT n FROM (SELECT 1 AS n) x(renamed)")]
#[case::union("SELECT n FROM (SELECT 1 AS n UNION ALL SELECT 2 AS n) x(renamed)")]
fn hidden_original_name_is_unresolved(#[case] sql: &str) {
    let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert_eq!(context.reported_problem_count(), 1);
    assert_eq!(
        context.reported_problems()[0].code(),
        "column_not_found_in_source"
    );
    assert_matches::assert_matches!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression(),
        Some(SemScalarExpr::ColumnRef(
            ColumnReferenceExpr::UndefinedColumn(_)
        ))
    );
}

#[test]
fn competing_derived_aliases_are_ambiguous() {
    let (query, context) = resolve(
        "SELECT renamed FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y(renamed)",
        Dialect::postgres(),
        false,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 1);
    assert_eq!(context.reported_problems()[0].code(), "ambiguous_column");
    assert_matches::assert_matches!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression(),
        Some(SemScalarExpr::ColumnRef(
            ColumnReferenceExpr::AmbiguousColumnRef(_)
        ))
    );
}

#[rstest::rstest]
#[case::non_null("1", true)]
#[case::nullable("NULL::int4", false)]
fn renamed_column_retains_type_and_nullability(#[case] value: &str, #[case] not_null: bool) {
    let sql = format!("SELECT renamed FROM (SELECT {value} AS n) x(renamed)");
    let (query, context) = resolve(&sql, Dialect::postgres(), false).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(
        column.column_name(),
        &Some(crate::sem::ColumnName::from("renamed"))
    );
    assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
    assert_eq!(column.is_not_null(), &Some(not_null));
    assert_matches::assert_matches!(
        column.expression(),
        Some(SemScalarExpr::ColumnRef(
            ColumnReferenceExpr::SubQueryColumn(_)
        ))
    );
}
