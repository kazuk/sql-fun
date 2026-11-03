//! EXISTS behavior at the query-analysis boundary using a small catalog.

use crate::test_helpers::query::expression as projection;

use crate::ast::Dialect;
use crate::context::ParseContext;
use crate::names::ColumnName;
use crate::sem::AnalysisError;
use crate::sem::PgBuiltInType;
use crate::sem::SemScalarExpr;
use crate::test_helpers::query::resolve;

#[rstest::rstest]
#[case::null("SELECT EXISTS (SELECT NULL) AS found")]
#[case::not_null("SELECT NOT EXISTS (SELECT NULL) AS found")]
#[case::empty("SELECT EXISTS (SELECT NULL WHERE false) AS found")]
#[case::not_empty("SELECT NOT EXISTS (SELECT NULL WHERE false) AS found")]
#[case::integer("SELECT EXISTS (SELECT 42) AS found")]
#[case::text("SELECT EXISTS (SELECT 'hello') AS found")]
#[case::many_columns("SELECT EXISTS (SELECT 1, NULL) AS found")]
#[case::no_columns("SELECT EXISTS (SELECT) AS found")]
#[case::nested("SELECT EXISTS (SELECT EXISTS (SELECT NULL)) AS found")]
#[case::boolean("SELECT NOT EXISTS (SELECT NULL) AND EXISTS (SELECT 1) AS found")]
#[case::enclosing("SELECT found FROM (SELECT EXISTS (SELECT NULL) AS found) s")]
fn exists_is_non_null_boolean(#[case] sql: &str, #[values(false, true)] infer: bool) {
    let (query, context) = resolve(sql, Dialect::postgres(), infer).unwrap();
    assert_eq!(context.reported_problem_count(), 0, "{sql}");
    let column = query
        .get_result_column_def(&ColumnName::from("found"))
        .unwrap();
    assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::bool()));
    assert_eq!(column.is_not_null(), Some(true));
}

#[rstest::rstest]
#[case::projection("SELECT EXISTS (SELECT $1::integer) AS found")]
#[case::inferred_projection("SELECT EXISTS (SELECT $1 + 1) AS found")]
#[case::predicate("SELECT EXISTS (SELECT NULL WHERE $1::integer < 2) AS found")]
#[case::not("SELECT NOT EXISTS (SELECT $1::integer) AS found")]
#[case::boolean("SELECT EXISTS (SELECT $1::integer) AND true AS found")]
#[case::many_columns("SELECT EXISTS (SELECT $1::integer, NULL) AS found")]
#[case::nested("SELECT EXISTS (SELECT EXISTS (SELECT $1::integer)) AS found")]
#[case::enclosing("SELECT found FROM (SELECT EXISTS (SELECT $1::integer) AS found) s")]
fn child_parameter_is_independent_of_boolean_result(#[case] sql: &str) {
    let (mut query, context) = resolve(sql, Dialect::postgres(), true).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{sql}: {:?}",
        context.reported_problems()
    );
    let column = query
        .get_result_column_def(&ColumnName::from("found"))
        .unwrap();
    assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::bool()));
    assert_eq!(column.is_not_null(), Some(true));
    assert_eq!(
        context
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .single_type(),
        Some(PgBuiltInType::int4())
    );
    let mut parameters = Parameters::default();
    query.walk(&mut parameters).unwrap();
    assert_eq!(parameters.0, vec![1]);
}

#[derive(Default)]
struct Parameters(Vec<u32>);

impl crate::sem::traversal::SemanticVisitorMut for Parameters {
    fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if let SemScalarExpr::Param(parameter) = value {
            self.0.push(parameter.position());
        }
        Ok(())
    }
}

#[rstest::rstest]
fn retains_correlated_query_and_serialized_predicate(#[values(false, true)] infer: bool) {
    let (query, context) = resolve(
        "SELECT EXISTS (SELECT 1 WHERE s.n IS NULL) AS found FROM (SELECT 1 AS n) s",
        Dialect::postgres(),
        infer,
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let expression = projection(&query);
    assert_eq!(expression.get_column_def(), None);
    assert!(expression.get_column_name().is_empty());
    let exists = expression.as_exists().expect("explicit EXISTS semantics");
    assert_eq!(exists.query().result_columns().len(), 1);
    let serialized = ::serde_json::to_string(expression).unwrap();
    let mut restored: SemScalarExpr = ::serde_json::from_str(&serialized).unwrap();
    assert!(restored.as_exists().is_some());
    assert_eq!(restored.get_type(), Some(PgBuiltInType::bool()));
    assert_eq!(restored.is_not_null(), Some(true));
    let mut bindings = Bindings::default();
    restored.walk(&mut bindings).unwrap();
    assert_eq!(bindings.correlated, 1);
    assert_eq!(bindings.missing, 0);
    assert_eq!(bindings.ambiguous, 0);
}

#[rstest::rstest]
#[case::missing("SELECT EXISTS (SELECT missing) AS found", 1, 0)]
#[case::ambiguous(
    "SELECT EXISTS (SELECT n FROM (SELECT 1 AS n) a, (SELECT 2 AS n) b) AS found",
    0,
    1
)]
fn retains_failed_child_bindings(
    #[case] sql: &str,
    #[case] missing: usize,
    #[case] ambiguous: usize,
) {
    let (mut query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
    assert!(context.reported_problem_count() > 0);
    assert!(projection(&query).as_exists().is_some());
    let mut bindings = Bindings::default();
    query.walk(&mut bindings).unwrap();
    assert_eq!(bindings.missing, missing);
    assert_eq!(bindings.ambiguous, ambiguous);
}

#[derive(Default)]
struct Bindings {
    correlated: usize,
    missing: usize,
    ambiguous: usize,
}

impl crate::sem::traversal::SemanticVisitorMut for Bindings {
    fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        use crate::sem::ColumnReferenceExpr;
        match value {
            SemScalarExpr::ColumnRef(ColumnReferenceExpr::SubQueryColumn(reference)) => {
                assert_eq!(::serde_json::to_value(reference).unwrap()["alias"], "s");
                self.correlated += 1;
            }
            SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedColumn(_)) => self.missing += 1,
            SemScalarExpr::ColumnRef(ColumnReferenceExpr::AmbiguousColumnRef(_)) => {
                self.ambiguous += 1
            }
            _ => {}
        }
        Ok(())
    }
}

#[test]
fn scalar_subquery_remains_nullable_integer() {
    let (query, context) =
        resolve("SELECT (SELECT 1 AS n) AS value", Dialect::postgres(), true).unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    let column = query
        .get_result_column_def(&ColumnName::from("value"))
        .unwrap();
    assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
    assert_eq!(column.is_not_null(), Some(false));
    assert!(projection(&query).as_exists().is_none());
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::null("SELECT EXISTS (SELECT NULL) AS found", 1)]
#[case::not("SELECT NOT EXISTS (SELECT NULL) AS found", 0)]
#[case::empty("SELECT EXISTS (SELECT NULL WHERE false) AS found", 0)]
#[case::not_empty("SELECT NOT EXISTS (SELECT NULL WHERE false) AS found", 1)]
#[case::many_columns("SELECT EXISTS (SELECT 1, NULL) AS found", 1)]
fn sqlite_exists_uses_shared_semantics(#[case] sql: &str, #[case] expected: i32) {
    let (query, context) = resolve(sql, Dialect::sqlite(), true).unwrap();
    assert_eq!(context.reported_problem_count(), 0, "{sql}");
    let column = query
        .get_result_column_def(&ColumnName::from("found"))
        .unwrap();
    assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::bool()));
    assert_eq!(column.is_not_null(), Some(true));
    let connection = ::rusqlite::Connection::open_in_memory().unwrap();
    let result: i32 = connection.query_row(sql, [], |row| row.get(0)).unwrap();
    assert_eq!(result, expected);
}
