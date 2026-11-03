#[cfg(feature = "sqlite")]
use crate::test_helpers::membership::context_for;
use crate::{
    ast::Dialect,
    parse_statements,
    sem::{ParseContext, PgBuiltInType, SemScalarExpr},
    test_helpers::{
        create_cache_context,
        membership::{composite_schema, context, expression, query, schema},
    },
};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::list("SELECT 1 IN (1,2)", false, false)]
#[case::negative_list("SELECT 1 NOT IN (1,2)", true, false)]
#[case::query("SELECT 1 IN (SELECT 1)", false, true)]
#[case::parameter_list("SELECT $1 IN (1,2)", false, false)]
#[case::parameter_query("SELECT $1 IN (SELECT 1)", false, true)]
#[case::row_list("SELECT ROW(1,2) IN (ROW(1,2),ROW(3,4))", false, false)]
#[case::row_query("SELECT ROW(1,2) IN (SELECT 1,2)", false, true)]
fn retains_operands_and_bound_comparisons(
    #[case] sql: &str,
    #[case] negated: bool,
    #[case] subquery: bool,
) -> testresult::TestResult {
    let (query, context) = query(sql, context())?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let value = expression(&query).as_membership().unwrap();
    assert_eq!(value.negated(), negated);
    assert_eq!(value.subquery().is_some(), subquery);
    assert!(value.binding().unwrap().resolution().is_resolved());
    assert!(!value.comparisons().is_empty());
    for comparison in value.comparisons() {
        assert!(
            comparison
                .expression()
                .unwrap()
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
    }
    let encoded = ::serde_json::to_string(expression(&query))?;
    let decoded: SemScalarExpr = ::serde_json::from_str(&encoded)?;
    assert_eq!(decoded.as_membership().unwrap().negated(), negated);
    Ok(())
}

#[rstest::rstest]
#[case::list("SELECT $1 IN (1,2)")]
#[case::query("SELECT $1 IN (SELECT 1)")]
#[case::rhs_query("SELECT 1 IN (SELECT $1)")]
#[case::row("SELECT ROW($1,2) IN (SELECT 1,2)")]
#[case::repeated("SELECT $1 IN (1), $1 NOT IN (2)")]
fn propagates_parameter_constraints(#[case] sql: &str) -> testresult::TestResult {
    let (query, context) = query(sql, context())?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let constraints = context.parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::bool()));
    Ok(())
}

#[rstest::rstest]
#[case::list("SELECT $1 IN ($2)")]
#[case::query("SELECT $1 IN (SELECT $2)")]
#[case::common_list("SELECT $1 IN ($2,$3)")]
fn preserves_parameter_correlations(#[case] sql: &str) -> testresult::TestResult {
    let (_, context) = query(sql, context())?;
    let constraints = context.parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::int4())
    ])));
    assert!(!constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::bool())
    ])));
    Ok(())
}

#[rstest::rstest]
#[case::query_width("SELECT 1 IN (SELECT 1,2)", "membership_width")]
#[case::row_width("SELECT ROW(1,2) IN (ROW(1))", "membership_width")]
#[case::scalar_row("SELECT 1 IN (ROW(1))", "binary_operator_overload_resolution_failed")]
#[case::row_scalar("SELECT ROW(1) IN (1)", "binary_operator_overload_resolution_failed")]
#[case::type_error("SELECT 1 IN (true)", "binary_operator_overload_resolution_failed")]
fn retains_rejected_membership(#[case] sql: &str, #[case] code: &str) -> testresult::TestResult {
    let (query, context) = query(sql, context())?;
    assert!(
        context.problems().iter().any(|p| p.code() == code),
        "{:?}",
        context.problems()
    );
    assert!(expression(&query).as_membership().is_some());
    Ok(())
}

#[test]
fn binds_correlated_columns() -> testresult::TestResult {
    let context = schema("CREATE TABLE flags(id integer NOT NULL)")?;
    let (query, context) = query("SELECT id IN (SELECT flags.id) FROM flags", context)?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let value = expression(&query).as_membership().unwrap();
    assert!(value.left().get_column_def().is_some());
    assert!(
        value
            .subquery()
            .unwrap()
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .get_column_def()
            .is_some()
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::empty("SELECT NULL IN ()", 0)]
#[case::negative_empty("SELECT NULL NOT IN ()", 1)]
#[case::list("SELECT 1 IN (1,NULL)", 1)]
#[case::query("SELECT 1 IN (SELECT 1)", 1)]
#[case::row("SELECT (1,2) IN (SELECT 1,2)", 1)]
#[case::negative_query("SELECT 1 NOT IN (SELECT 2)", 1)]
#[case::row_list("SELECT (1,2) IN ((1,2),(3,4))", 1)]
fn sqlite_membership_has_integer_result(
    #[case] sql: &str,
    #[case] expected: i64,
) -> testresult::TestResult {
    let db = ::rusqlite::Connection::open_in_memory()?;
    assert_eq!(db.query_row(sql, [], |row| row.get::<_, i64>(0))?, expected);
    let (query, context) = query(sql, context_for(Dialect::sqlite()))?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
    Ok(())
}

#[rstest::rstest]
#[case::one("singleton", true)]
#[case::two("pair", false)]
fn deferred_star_validates_final_query_width(
    #[case] name: &str,
    #[case] valid: bool,
) -> testresult::TestResult {
    let context = composite_schema()?;
    let (query, context) = query(
        &format!("SELECT 1 IN (SELECT ($1).*) AS result, take_{name}($1)"),
        context,
    )?;
    let predicate = expression(&query).as_membership().unwrap();
    assert!(!predicate.subquery().unwrap().has_pending_stars());
    assert_eq!(
        predicate.types().admissible_types() == Some(&[][..]),
        !valid
    );
    assert_eq!(
        context
            .problems()
            .iter()
            .any(|p| p.code() == "membership_width"),
        !valid,
        "{:?}",
        context.problems()
    );
    Ok(())
}

#[test]
fn rejection_does_not_stop_following_statement() -> testresult::TestResult {
    let mut cache_context = create_cache_context();
    let mut context = context();
    let mut statements = Vec::new();
    for statement in parse_statements(
        "SELECT 1 IN (SELECT 1,2); SELECT 1 IN (1)",
        Dialect::postgres(),
    ) {
        let analyzed = statement?.analyze(context, &mut cache_context)?;
        statements.push(analyzed.ast().clone().into_select_statemtnt().unwrap());
        context = analyzed.into_context();
    }
    assert_eq!(expression(&statements[0]).get_type(), None);
    assert_eq!(
        expression(&statements[1]).get_type(),
        Some(PgBuiltInType::bool())
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_membership_does_not_impose_operand_affinity() -> testresult::TestResult {
    let (_, context) = query("SELECT $1 IN ($2)", context_for(Dialect::sqlite()))?;
    assert!(context.parameter_constraints().allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::text())
    ])));
    Ok(())
}

#[rstest::rstest]
#[case::list("SELECT NULL IN (1, TRUE)")]
#[case::negative_list("SELECT NULL NOT IN (1, TRUE)")]
fn heterogeneous_comparisons_retain_independent_coercions(
    #[case] sql: &str,
) -> testresult::TestResult {
    let (query, context) = query(sql, context())?;
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let predicate = expression(&query).as_membership().unwrap();
    assert_eq!(predicate.comparisons().len(), 2);
    for (comparison, ty) in predicate
        .comparisons()
        .iter()
        .zip([PgBuiltInType::int4(), PgBuiltInType::bool()])
    {
        assert_eq!(comparison.left().get_type(), Some(ty.clone()));
        assert_eq!(comparison.right().get_type(), Some(ty));
        assert!(
            comparison
                .expression()
                .unwrap()
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
    }
    Ok(())
}
