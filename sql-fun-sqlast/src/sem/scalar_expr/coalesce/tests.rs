//! Small-catalog semantic and failure-continuation contracts.
mod test_helpers;

use crate::{
    ast::Dialect,
    parse_statements,
    sem::{ParseContext, PgBuiltInType},
};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::mixed("SELECT COALESCE($1,1,true)")]
#[case::unreachable_mixed("SELECT COALESCE(1,$1,true)")]
#[case::contradictory("SELECT COALESCE($1,1) WHERE $1")]
fn failures_retain_operands_and_allow_next_statement(#[case] sql: &str) -> testresult::TestResult {
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
    let coalesce = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_coalesce()
        .unwrap();
    assert!(coalesce.arguments().len() >= 2);
    assert_eq!(
        coalesce.types().unwrap().admissible_types(),
        Some([].as_slice())
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

#[test]
fn an_unreachable_parameter_still_constrains_the_result() -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements("SELECT COALESCE(1,$1)", Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context(), &mut cache_context)?;
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
    let warning = analyzed
        .context()
        .problems()
        .iter()
        .find(|p| p.code() == "unreachable_coalesce_argument")
        .unwrap();
    assert_eq!(
        warning.severity(),
        ::sql_fun_core::analysis_problem::DiagnosticSeverity::Warning
    );
    assert!(warning.render_metadata().is_some());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::heterogeneous("SELECT COALESCE(?1,1,'text')")]
#[case::case_insensitive("SELECT coalesce(?1,1)")]
#[case::null("SELECT COALESCE(?1,NULL)")]
fn sqlite_does_not_impose_postgres_common_types(#[case] sql: &str) -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements(sql, Dialect::sqlite())
        .next()
        .unwrap()?
        .analyze(test_helpers::sqlite_context(), &mut cache_context)?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .all(|p| p.code() == "unreachable_coalesce_argument"),
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
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type()
            .is_none()
    );
    assert_eq!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_coalesce()
            .unwrap()
            .arguments()
            .len(),
        if sql.contains("text") { 3 } else { 2 }
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_requires_two_arguments_without_losing_the_expression() -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements("SELECT COALESCE(?1)", Dialect::sqlite())
        .next()
        .unwrap()?
        .analyze(test_helpers::sqlite_context(), &mut cache_context)?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|p| p.code() == "parameter_constraint")
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_coalesce()
            .unwrap()
            .types()
            .unwrap()
            .admissible_types(),
        Some([].as_slice())
    );
    Ok(())
}

#[rstest::rstest]
#[case::multiple("SELECT COALESCE($1,$2)", 2)]
#[case::single("SELECT COALESCE($1)", 1)]
#[case::null("SELECT COALESCE($1,NULL)", 2)]
fn untyped_operands_retain_the_semantic_expression(
    #[case] sql: &str,
    #[case] count: usize,
) -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context(), &mut cache_context)?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let coalesce = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_coalesce()
        .unwrap();
    assert_eq!(coalesce.arguments().len(), count);
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .parameter(1)
            .is_some()
    );
    // Do not assert the premature text narrowing tracked separately in #676.
    Ok(())
}

#[test]
fn incompatible_concrete_operands_retain_a_failed_type_domain() -> testresult::TestResult {
    let mut cache_context = crate::CacheContext::new();
    let analyzed = parse_statements("SELECT COALESCE(1,true)", Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(test_helpers::context(), &mut cache_context)?;
    let failure = analyzed
        .context()
        .problems()
        .iter()
        .find(|p| p.code() == "coalesce_expr_type_unknown")
        .unwrap();
    assert!(failure.render_metadata().is_some());
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let coalesce = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_coalesce()
        .unwrap();
    assert_eq!(
        coalesce.types().unwrap().admissible_types(),
        Some([].as_slice())
    );
    assert_eq!(coalesce.arguments().len(), 2);
    Ok(())
}
