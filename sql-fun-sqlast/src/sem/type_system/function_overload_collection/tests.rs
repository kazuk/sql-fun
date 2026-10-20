//! Function selection through SQL analysis, with a small explicit catalog.
mod test_helpers;

use crate::sem::{ParseContext, PgBuiltInType, TypeReference};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::simple("anyelement", "integer", "f(1)", "int4")]
#[case::preferred_conversion("anyelement", "double precision", "f(1)", "int4")]
#[case::common("anycompatible", "integer", "f(1)", "int4")]
#[case::nonarray("anynonarray", "integer", "f(1)", "int4")]
#[case::compatible_nonarray("anycompatiblenonarray", "integer", "f(1)", "int4")]
#[case::array("anyarray", "integer[]", "f(NULL::integer[])", "int4")]
#[case::common_array("anycompatiblearray", "integer[]", "f(NULL::integer[])", "int4")]
#[case::named_array("anyarray", "control[]", "f(NULL::control[])", "int4")]
#[case::constructed_array("anyarray", "control[]", "f(ARRAY[NULL::control])", "int4")]
#[case::partial("anyelement, integer", "integer, bigint", "f(1,1::smallint)", "int4")]
#[case::polymorphic_wins("anyelement, integer", "bigint, bigint", "f(1,1)", "text")]
#[case::polymorphic_only_applies("anyelement", "boolean", "f(1)", "text")]
#[case::defaulted(
    "anyelement, integer DEFAULT 1",
    "integer, bigint DEFAULT 1",
    "f(1)",
    "int4"
)]
#[case::expanded("VARIADIC anyarray", "integer", "f(1)", "int4")]
#[case::nonvariadic("VARIADIC integer[]", "integer", "f(1)", "int4")]
fn declared_inputs_determine_selection(
    #[case] polymorphic: &str,
    #[case] concrete: &str,
    #[case] call: &str,
    #[case] result: &str,
    #[values(false, true)] reversed: bool,
) -> testresult::TestResult {
    let analyzed =
        test_helpers::analyze(&format!("SELECT {call}"), polymorphic, concrete, reversed)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    let expected = TypeReference::from_master_type_name(result);
    assert_eq!(column.col_type(), &Some(expected.clone()));
    let function = column.expression().unwrap().as_function_call().unwrap();
    assert_eq!(
        function.overload().as_ref().unwrap().scaler_ret_type(),
        Some(expected)
    );
    Ok(())
}

#[rstest::rstest]
#[case::typed_parameter("anyelement, integer", "integer, integer", "f(1,$1::integer)", "int4")]
#[case::typed_parameter_poly("anyelement, integer", "bigint, bigint", "f(1,$1::integer)", "text")]
#[case::typed_parameter_concrete(
    "anyelement, integer",
    "bigint, bigint",
    "f(1,$1::bigint)",
    "int4"
)]
fn explicitly_typed_parameters_agree_with_immediate_selection(
    #[case] polymorphic: &str,
    #[case] concrete: &str,
    #[case] call: &str,
    #[case] result: &str,
    #[values(false, true)] reversed: bool,
) -> testresult::TestResult {
    let analyzed =
        test_helpers::analyze(&format!("SELECT {call}"), polymorphic, concrete, reversed)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let expected = TypeReference::from_master_type_name(result);
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(expected.clone())
    );
    assert_eq!(
        analyzed
            .context()
            .parameter_constraints()
            .result_column(0)
            .unwrap()
            .admissible_types(),
        Some(std::slice::from_ref(&expected))
    );
    Ok(())
}

#[rstest::rstest]
#[case::equal_partial("anyelement, integer", "integer, bigint", "f(1,1)")]
#[case::unknown("integer", "bigint", "f(NULL)")]
#[case::defaults("integer, integer DEFAULT 1", "integer, bigint DEFAULT 1", "f(1)")]
fn genuine_ambiguity_is_preserved(
    #[case] left: &str,
    #[case] right: &str,
    #[case] call: &str,
    #[values(false, true)] reversed: bool,
) -> testresult::TestResult {
    let analyzed = test_helpers::analyze(&format!("SELECT {call}"), left, right, reversed)?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| { problem.code() == "function_overload_resolution_failed" }),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let function = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_function_call()
        .unwrap();
    assert!(function.overload().is_none());
    Ok(())
}

#[rstest::rstest]
#[case::simple("anyelement")]
#[case::common("anycompatible")]
fn deferred_inputs_are_ranked_per_source_assignment(
    #[case] polymorphic: &str,
    #[values(false, true)] reversed: bool,
) -> testresult::TestResult {
    let analyzed = test_helpers::analyze(
        "SELECT f(1,$1)",
        &format!("{polymorphic}, integer"),
        "bigint, bigint",
        reversed,
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int8())])));
    // Neither declaration has an exact input for int2: this assignment ties.
    assert!(!constraints.allows(&BTreeMap::from([(
        1,
        TypeReference::from_master_type_name("int2")
    )])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    let result = constraints.result_column(0).unwrap();
    assert!(!result.is_any());
    assert!(result.allows(&PgBuiltInType::text()));
    assert!(result.allows(&PgBuiltInType::int4()));
    assert!(!result.allows(&PgBuiltInType::int8()));
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type()
            .is_none()
    );
    Ok(())
}

#[rstest::rstest]
#[case::simple("anyelement")]
#[case::common("anycompatible")]
fn deferred_exact_match_keeps_concrete_result(
    #[case] polymorphic: &str,
    #[values(false, true)] reversed: bool,
) -> testresult::TestResult {
    let analyzed = test_helpers::analyze(
        "SELECT f(1,$1)",
        &format!("{polymorphic}, integer"),
        "integer, integer",
        reversed,
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    let result = constraints.result_column(0).unwrap();
    assert!(result.allows(&PgBuiltInType::int4()));
    assert!(!result.allows(&PgBuiltInType::text()));
    Ok(())
}

#[rstest::rstest]
#[case::polymorphic_result("accept_text", "int4", "int8", "text")]
#[case::concrete_result("accept_integer", "int8", "int4", "int4")]
fn deferred_results_remain_correlated(
    #[case] consumer: &str,
    #[case] admitted: &str,
    #[case] rejected: &str,
    #[case] result: &str,
    #[values(false, true)] reversed: bool,
) -> testresult::TestResult {
    let analyzed = test_helpers::analyze(
        &format!("SELECT {consumer}(f(1,$1))"),
        "anyelement, integer",
        "bigint, bigint",
        reversed,
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(
        1,
        TypeReference::from_master_type_name(admitted)
    )])));
    assert!(!constraints.allows(&BTreeMap::from([(
        1,
        TypeReference::from_master_type_name(rejected)
    )])));
    assert!(
        constraints
            .result_column(0)
            .unwrap()
            .allows(&TypeReference::from_master_type_name(result))
    );
    Ok(())
}
