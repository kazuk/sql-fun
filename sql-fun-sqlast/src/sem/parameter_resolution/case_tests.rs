//! CASE contracts at the public analysis boundary with a small explicit catalog.
mod test_helpers;

use crate::{
    ast::Dialect,
    parse_statements,
    sem::{ParseContext, PgBuiltInType},
};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::searched("SELECT CASE WHEN $1 THEN 1 ELSE 2 END")]
#[case::multiple_arms("SELECT CASE WHEN $1 THEN 1 WHEN false THEN 2 ELSE 3 END")]
#[case::repeated_condition("SELECT CASE WHEN $1 THEN 1 WHEN $1 THEN 2 ELSE 3 END")]
fn searched_conditions_require_boolean(#[case] sql: &str) -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context(), &mut cache_context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
    assert_eq!(column.is_not_null(), &Some(true));
    Ok(())
}

#[rstest::rstest]
#[case::unconstrained_predicate("SELECT CASE WHEN $1 IS NULL THEN 1 ELSE 2 END")]
#[case::outer_parameter("SELECT $1 IS NULL, CASE WHEN $1 IS NULL THEN 1 ELSE 2 END")]
fn null_test_does_not_choose_an_input_type(#[case] sql: &str) -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context(), &mut cache_context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.parameter(1).unwrap().constraint().is_any());
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    Ok(())
}

#[rstest::rstest]
#[case::selector("SELECT CASE $1 WHEN 1 THEN 2 ELSE 3 END")]
#[case::when_operand("SELECT CASE 1 WHEN $1 THEN 2 ELSE 3 END")]
#[case::typed_result("SELECT CASE 1 WHEN 1 THEN $1::integer ELSE 3 END")]
fn simple_case_keeps_the_resolved_comparison(#[case] sql: &str) -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context(), &mut cache_context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let case = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_case()
        .unwrap();
    assert!(case.argument().is_some());
    assert_eq!(
        case.arms()[0].comparison().unwrap().get_type(),
        Some(PgBuiltInType::bool())
    );
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::int4())]))
    );
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
    );
    Ok(())
}

#[rstest::rstest]
#[case::non_boolean("SELECT CASE WHEN 1 THEN $1 ELSE 1 END")]
#[case::mixed_results("SELECT CASE WHEN $1 THEN 1 ELSE true END")]
#[case::contradictory("SELECT CASE WHEN $1 THEN $1 ELSE 1 END")]
fn failures_retain_case_and_allow_next_statement(#[case] sql: &str) -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context(), &mut cache_context)?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|p| p.code() == "parameter_constraint"),
        "{:?}",
        analyzed.context().problems()
    );
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::new())
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_case()
            .unwrap()
            .arms()
            .len(),
        1
    );
    let next = parse_statements("SELECT 1", Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(analyzed.into_context(), &mut cache_context)?;
    assert!(
        next.context()
            .parameter_constraints()
            .allows(&BTreeMap::new())
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::integer_condition("SELECT CASE WHEN 1 THEN ?1 ELSE 2 END")]
#[case::selector("SELECT CASE ?1 WHEN 1 THEN 2 ELSE 3 END")]
fn sqlite_does_not_use_postgres_condition_constraints(#[case] sql: &str) -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements(sql, Dialect::sqlite())
        .next()
        .unwrap()?
        .analyze(test_helpers::sqlite_context(), &mut cache_context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .is_any()
    );
    assert!(
        analyzed
            .ast()
            .clone()
            .into_select_statemtnt()
            .unwrap()
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_case()
            .is_some()
    );
    Ok(())
}

#[rstest::rstest]
#[case::array_else_domain(
    "CASE WHEN true THEN NULL::integer[] ELSE NULL::amount[] END",
    "public.amount[]",
    false
)]
#[case::array_else_base(
    "CASE WHEN true THEN NULL::amount[] ELSE NULL::integer[] END",
    "pg_catalog.int4[]",
    false
)]
#[case::scalar_domain(
    "CASE WHEN true THEN NULL::integer ELSE NULL::amount END",
    "pg_catalog.int4",
    false
)]
#[case::unknown_array(
    "CASE WHEN true THEN NULL ELSE NULL::amount[] END",
    "public.amount[]",
    false
)]
#[case::domain_over_array(
    "CASE WHEN true THEN NULL::integer[] ELSE NULL::amount_array END",
    "pg_catalog.int4[]",
    false
)]
#[case::simple(
    "CASE 1 WHEN 1 THEN NULL::integer[] ELSE NULL::amount[] END",
    "public.amount[]",
    false
)]
#[case::multiple(
    "CASE WHEN true THEN NULL::integer[] WHEN false THEN NULL::amount[] ELSE NULL::amount[] END",
    "public.amount[]",
    false
)]
#[case::identical_domain(
    "CASE WHEN true THEN NULL::amount ELSE NULL::amount END",
    "public.amount",
    false
)]
#[case::identical_array(
    "CASE WHEN true THEN NULL::amount[] ELSE NULL::amount[] END",
    "public.amount[]",
    false
)]
#[case::string_unknown("CASE WHEN true THEN '1' ELSE 2 END", "pg_catalog.int4", true)]
#[case::all_unknown("CASE WHEN true THEN NULL ELSE NULL END", "pg_catalog.text", false)]
#[case::omitted_else("CASE WHEN true THEN NULL::amount END", "pg_catalog.int4", false)]
#[case::nested(
    "CASE WHEN true THEN CASE WHEN false THEN NULL::integer[] ELSE NULL::amount[] END ELSE NULL::amount[] END",
    "public.amount[]",
    false
)]
#[case::parent_operator("(CASE WHEN true THEN 1 ELSE 2 END) + 1", "pg_catalog.int4", false)]
#[case::array_null_else(
    "CASE WHEN true THEN NULL::amount[] ELSE NULL END",
    "public.amount[]",
    false
)]
#[case::array_omitted_else("CASE WHEN true THEN NULL::amount[] END", "public.amount[]", false)]
#[case::parameter_condition(
    "CASE WHEN $1 THEN NULL::integer[] ELSE NULL::amount[] END",
    "public.amount[]",
    false
)]
fn known_results_use_ordered_common_types(
    #[case] expression: &str,
    #[case] expected: &str,
    #[case] not_null: bool,
) -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements(&format!("SELECT {expression}"), Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context(), &mut cache_context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type(), &Some(expected.parse()?));
    assert_eq!(column.is_not_null(), &Some(not_null));
    if let Some(case) = column.expression().unwrap().as_case() {
        assert_eq!(case.arms()[0].result().get_type(), Some(expected.parse()?));
        if let Some(otherwise) = case.otherwise() {
            assert_eq!(otherwise.get_type(), Some(expected.parse()?));
        }
    }
    Ok(())
}

#[rstest::rstest]
#[case::standalone("SELECT CASE WHEN true THEN 1 ELSE false END")]
#[case::unrelated_parameter("SELECT CASE WHEN true THEN 1 ELSE false END, $1::integer")]
fn incompatible_known_results_keep_specific_diagnostic(
    #[case] sql: &str,
) -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context(), &mut cache_context)?;
    let problems = analyzed.context().problems();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].code(), "incompatible_case_values");
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert!(column.col_type().is_none());
    assert!(column.expression().unwrap().as_case().is_some());
    let next = parse_statements("SELECT 1", Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(analyzed.into_context(), &mut cache_context)?;
    assert!(
        next.context()
            .parameter_constraints()
            .allows(&BTreeMap::new())
    );
    Ok(())
}
