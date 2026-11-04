//! Public query analysis retains qualified identity and semantic failure outcomes.
use crate::{
    names::OperatorName,
    sem::{PgBuiltInType, QueryNameResolution},
    test_helpers::{
        operator_names::{addition_context, analyze},
        query::expression,
    },
};

#[rstest::rstest]
#[case("SELECT 1 + 2", None)]
#[case("SELECT 1 OPERATOR(pg_catalog.+) 2", Some("pg_catalog"))]
#[case(r#"SELECT 1 OPERATOR("Mixed".+) 2"#, Some("Mixed"))]
#[case(r#"SELECT 1 OPERATOR("a.b".+) 2"#, Some("a.b"))]
#[case(r#"SELECT 1 OPERATOR("a""b".+) 2"#, Some("a\"b"))]
#[case("SELECT $1::int4 OPERATOR(pg_catalog.+) 2", Some("pg_catalog"))]
fn qualified_operator_binding_survives_inference(
    #[case] sql: &str,
    #[case] schema: Option<&str>,
) -> testresult::TestResult {
    let (query, context) = analyze(sql, addition_context(schema))?;
    let value = expression(&query);
    assert_eq!(context.reported_problem_count(), 0);
    assert_eq!(value.get_type(), Some(PgBuiltInType::int4()));
    assert_eq!(
        value.operator_binding().unwrap().resolution(),
        &QueryNameResolution::operator(&OperatorName::from_parts(
            Some(schema.unwrap_or("pg_catalog")),
            "+"
        ))
    );
    Ok(())
}

#[rstest::rstest]
#[case("SELECT 1 OPERATOR(a.b.+) 2")]
#[case("SELECT 1 OPERATOR(a.b.c.+) 2")]
#[case("SELECT $1::int4 OPERATOR(a.b.+) 2")]
fn cross_database_operator_is_recoverable_and_unresolved(
    #[case] sql: &str,
) -> testresult::TestResult {
    let (query, context) = analyze(sql, addition_context(None))?;
    assert_eq!(context.reported_problem_count(), 1);
    assert!(context.has_reported_problem("cross_database_operator_reference"));
    let value = expression(&query);
    assert!(value.get_type().is_none());
    assert_eq!(
        value.operator_binding().unwrap().resolution(),
        &QueryNameResolution::not_found()
    );
    // A later independent query still analyzes in the same environment.
    let (query, context) = analyze("SELECT 1 + 2", context)?;
    assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
    assert_eq!(context.reported_problem_count(), 1);
    Ok(())
}

#[rstest::rstest]
#[case("SELECT 1 OPERATOR(missing.+) 2")]
#[case(r#"SELECT 1 OPERATOR("PG_CATALOG".+) 2"#)]
fn missing_qualified_operator_does_not_bind_unqualified(
    #[case] sql: &str,
) -> testresult::TestResult {
    let (query, context) = analyze(sql, addition_context(None))?;
    assert!(context.has_reported_problem("undefined_operator"));
    assert!(!context.has_reported_problem("operator_name_syntax"));
    assert!(!context.has_reported_problem("cross_database_operator_reference"));
    assert_eq!(
        expression(&query).operator_binding().unwrap().resolution(),
        &QueryNameResolution::not_found()
    );
    Ok(())
}

#[test]
fn cross_database_ordering_operator_retains_failed_binding() -> testresult::TestResult {
    let (query, context) = analyze(
        "SELECT 1 + 2 ORDER BY 1 USING OPERATOR(a.b.<)",
        addition_context(None),
    )?;
    assert_eq!(context.reported_problem_count(), 1);
    assert!(context.has_reported_problem("cross_database_operator_reference"));
    assert_eq!(
        query.query_clauses().unwrap().order()[0]
            .using()
            .unwrap()
            .resolution(),
        &QueryNameResolution::not_found()
    );
    Ok(())
}
