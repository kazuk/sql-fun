//! Function selection through SQL analysis, with a small explicit catalog.
use crate::test_helpers::function_overloads as test_helpers;

use crate::sem::{ParseContext, PgBuiltInType, TypeReference};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::simple("anyelement", "integer", "f(1)", "int4")]
#[case::preferred_conversion("anyelement", "double precision", "f(1)", "int4")]
#[case::common("anycompatible", "integer", "f(1)", "int4")]
#[case::nonarray("anynonarray", "integer", "f(1)", "int4")]
#[case::compatible_nonarray("anycompatiblenonarray", "integer", "f(1)", "int4")]
#[case::vector("anyarray", "int2vector", "f(NULL::int2vector)", "int4")]
#[case::vector_common("anycompatiblearray", "int2vector", "f(NULL::int2vector)", "int4")]
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
#[case::heterogeneous(r#"VARIADIC "any""#, "integer", "f(1)", "int4")]
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

#[rstest::rstest]
#[case::element("anyarray", "anyelement", "vf(NULL::int2vector)", "int2")]
#[case::identity("anyarray", "anyarray", "vf(NULL::int2vector)", "int2vector")]
#[case::oid_identity("anyarray", "anyarray", "vf(NULL::oidvector)", "oidvector")]
#[case::nested("anyarray", "anyelement", "vf(NULL::int2vector[])", "int2vector")]
#[case::ordinary("anyarray", "anyarray", "vf(NULL::int4[])", "_int4")]
#[case::common(
    "anycompatiblearray",
    "anycompatiblearray",
    "vf(NULL::int2vector)",
    "_int2"
)]
#[case::typed_parameter("anyarray", "anyarray", "vf($1::int2vector)", "int2vector")]
fn vector_polymorphism_retains_binding_and_result(
    #[case] parameters: &str,
    #[case] result: &str,
    #[case] call: &str,
    #[case] expected: &str,
) -> testresult::TestResult {
    let analyzed = test_helpers::analyze_vector(&format!("SELECT {call}"), parameters, result)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    let expected = TypeReference::from_master_type_name(expected);
    assert_eq!(column.col_type(), &Some(expected.clone()));
    if let Some(function) = column.expression().unwrap().as_function_call() {
        assert_eq!(
            function.overload().as_ref().unwrap().scaler_ret_type(),
            Some(expected)
        );
    } else {
        assert_eq!(
            analyzed
                .context()
                .parameter_constraints()
                .result_column(0)
                .unwrap()
                .admissible_types(),
            Some(std::slice::from_ref(&expected))
        );
        let retained = ::serde_json::to_value(column.expression().unwrap())?;
        assert!(retained["Constrained"]["semantic_call"].is_object());
    }
    Ok(())
}

#[rstest::rstest]
#[case::nonarray("anynonarray", "vf(NULL::int2vector)")]
#[case::common_nonarray("anycompatiblenonarray", "vf(NULL::int2vector)")]
#[case::scalar("anyarray", "vf(1)")]
#[case::element_mismatch("anyarray, anyelement", "vf(NULL::int2vector, NULL::text)")]
#[case::array_identity_mismatch("anyarray, anyarray", "vf(NULL::int2vector, NULL::smallint[])")]
#[case::array_identity_mismatch_reversed(
    "anyarray, anyarray",
    "vf(NULL::smallint[], NULL::int2vector)"
)]
fn invalid_vector_polymorphism_retains_failure(
    #[case] parameters: &str,
    #[case] call: &str,
) -> testresult::TestResult {
    let analyzed = test_helpers::analyze_vector(&format!("SELECT {call}"), parameters, "text")?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|p| p.code() == "function_overload_resolution_failed")
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_function_call()
            .unwrap()
            .overload()
            .is_none()
    );
    Ok(())
}

#[test]
fn deferred_vector_element_preserves_array_result() -> testresult::TestResult {
    let analyzed = test_helpers::analyze_vector(
        "SELECT vf(NULL::int2vector, $1)",
        "anyarray, anyelement",
        "anyarray",
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(
        1,
        TypeReference::from_master_type_name("int2")
    )])));
    assert!(!constraints.allows(&BTreeMap::from([(
        1,
        TypeReference::from_master_type_name("text")
    )])));
    let expected = TypeReference::from_master_type_name("int2vector");
    assert_eq!(
        constraints.result_column(0).unwrap().admissible_types(),
        Some(std::slice::from_ref(&expected))
    );
    Ok(())
}

#[rstest::rstest]
#[case::name("format('%s'::text, 'abc'::name)", &["text", "name"])]
#[case::heterogeneous("format('%s %s %s'::text, 'abc'::name, 42, true)", &["text", "name", "int4", "bool"])]
#[case::array("format('%s'::text, ARRAY[1,2])", &["text", "int4[]"])]
#[case::int2vector("format('%s'::text, NULL::int2vector)", &["text", "int2vector"])]
#[case::oidvector("format('%s'::text, NULL::oidvector)", &["text", "oidvector"])]
#[case::typed_null("format('%s'::text, NULL::integer)", &["text", "int4"])]
#[case::zero_tail("format('literal'::text)", &["text"])]
fn variadic_any_preserves_independent_argument_types(
    #[case] call: &str,
    #[case] types: &[&str],
) -> testresult::TestResult {
    let analyzed = test_helpers::analyze_variadic_any(&format!("SELECT {call}"))?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type(), &Some(PgBuiltInType::text()));
    let function = column.expression().unwrap().as_function_call().unwrap();
    assert_eq!(
        function.overload().as_ref().unwrap().scaler_ret_type(),
        Some(PgBuiltInType::text())
    );
    assert_eq!(function.args().len(), types.len());
    for (argument, expected) in function.args().into_iter().zip(types) {
        let expected = if *expected == "int4[]" {
            PgBuiltInType::int4().array_type()
        } else {
            TypeReference::from_master_type_name(expected)
        };
        assert_eq!(argument.value().get_type(), Some(expected));
        assert!(argument.arg_definition().is_some());
    }
    Ok(())
}

#[rstest::rstest]
#[case::wrong_prefix("format(42, 'abc'::name)")]
#[case::missing_prefix("format()")]
fn variadic_any_does_not_relax_fixed_inputs(#[case] call: &str) -> testresult::TestResult {
    let analyzed = test_helpers::analyze_variadic_any(&format!("SELECT {call}"))?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|p| p.code() == "function_overload_resolution_failed")
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
#[case::untyped("SELECT format('%s'::text, $1)")]
#[case::typed("SELECT format('%s'::text, $1::integer)")]
#[case::independent("SELECT format('%s %s'::text, $1, $2)")]
fn variadic_any_parameter_domains(#[case] sql: &str) -> testresult::TestResult {
    let analyzed = test_helpers::analyze_variadic_any(sql)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    if !sql.contains("::integer") {
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4().array_type())])));
        assert!(constraints.allows(&BTreeMap::from([(
            1,
            TypeReference::from_full_name(crate::names::FullName::with_schema(
                "public",
                "unlisted_type"
            ))
        )])));
    }
    if sql.contains("$2") {
        assert!(constraints.allows(&BTreeMap::from([
            (1, PgBuiltInType::bool()),
            (2, PgBuiltInType::int4().array_type())
        ])));
    }
    assert_eq!(
        constraints.result_column(0).unwrap().single_type(),
        Some(PgBuiltInType::text())
    );
    Ok(())
}

#[rstest::rstest]
#[case::concrete("accept_integer", "int4", "bool", "int4")]
#[case::heterogeneous("accept_text", "bool", "int4", "text")]
fn deferred_variadic_any_keeps_concrete_overload_correlation(
    #[case] consumer: &str,
    #[case] admitted: &str,
    #[case] rejected: &str,
    #[case] result: &str,
    #[values(false, true)] reversed: bool,
) -> testresult::TestResult {
    let analyzed = test_helpers::analyze(
        &format!("SELECT {consumer}(f('x'::text,$1))"),
        r#"text, VARIADIC "any""#,
        "text, integer",
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
    assert_eq!(
        constraints.result_column(0).unwrap().single_type(),
        Some(TypeReference::from_master_type_name(result))
    );
    Ok(())
}
