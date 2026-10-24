//! Scalar ANY through PostgreSQL 18's shipped operator and cast metadata.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::sem::PgBuiltInType;

#[rstest::rstest]
#[case::integers("1 = ANY (ARRAY[1,2])")]
#[case::mixed("2::bigint = ANY (ARRAY[1,2]::integer[])")]
#[case::coerced_array("1::numeric = ANY (ARRAY[1,2]::real[])")]
#[case::empty("1 = ANY (ARRAY[]::integer[])")]
#[case::null_left_empty("NULL::integer = ANY (ARRAY[]::integer[])")]
#[case::null_elements("1 = ANY (ARRAY[NULL]::integer[])")]
#[case::null_array("1 = ANY (NULL::integer[])")]
#[case::multidimensional("1 = ANY (ARRAY[[1,2],[3,4]])")]
#[case::records("pg_control_system() = ANY (ARRAY[pg_control_system()])")]
fn typed_any_retains_boolean_and_binding(#[case] expression: &str) -> testresult::TestResult {
    // Actual builtin overload and cast families are the contract under test.
    let analyzed = heavy::analyze(&format!("SELECT {expression}"), false)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = select.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type(), &Some(PgBuiltInType::bool()));
    assert_eq!(column.is_not_null(), &None);
    let value = column.expression().unwrap();
    assert!(value.operator_binding().unwrap().resolution().is_resolved());
    let encoded = ::serde_json::to_value(value)?;
    let call = &encoded["Arith"]["OpAnyBinary"];
    assert_eq!(
        call["operator_def"]["result_type"],
        ::serde_json::to_value(PgBuiltInType::bool())?
    );
    assert!(!call["right_arg"].is_null());
    assert!(!call["left_arg"].is_null());
    Ok(())
}

#[rstest::rstest]
#[case::non_array("1 = ANY (1)", "require_array")]
#[case::incompatible(
    "1 = ANY (ARRAY['x']::text[])",
    "binary_operator_overload_resolution_failed"
)]
#[case::array_left(
    "ARRAY[1] = ANY (ARRAY[[1]])",
    "binary_operator_overload_resolution_failed"
)]
#[case::not_boolean("1 + ANY (ARRAY[1,2])", "undefined_operator")]
#[case::missing_operator("1 OPERATOR(public.===) ANY (ARRAY[1,2])", "undefined_operator")]
fn invalid_any_keeps_operands_and_failure(
    #[case] expression: &str,
    #[case] code: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(&format!("SELECT {expression}"), false)?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == code),
        "{:?}",
        analyzed.context().problems()
    );
    let diagnostic = analyzed
        .context()
        .problems()
        .iter()
        .find(|problem| problem.code() == "undefined_operator")
        .unwrap();
    let diagnostic = ::serde_json::to_value(diagnostic)?;
    assert_eq!(diagnostic["UndefinedOperator"][0], "AexprOpAny");
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = select.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type(), &None);
    let value = column.expression().unwrap();
    assert_eq!(
        value.operator_binding().unwrap().resolution().is_resolved(),
        !expression.contains("public.===")
    );
    let encoded = ::serde_json::to_value(value)?;
    let call = &encoded["Arith"]["UndefinedOperatorAny"];
    assert!(!call["larg"].is_null());
    assert!(!call["rarg"].is_null());
    Ok(())
}

#[test]
fn shipped_catalog_any_and_ordinary_array_equality_resolve() -> testresult::TestResult {
    // Full PostgreSQL 18 SQL catalog loading and pg_policy's real column types
    // are essential for the pg_policies expression regression (#735).
    let analyzed = heavy::analyze(
        "SELECT 1 = ANY (ARRAY[1,2]), pol.polroles = '{0}'::oid[] FROM pg_catalog.pg_policy pol",
        true,
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let any = select.result_columns().get_column_at(0).unwrap();
    assert_eq!(any.col_type(), &Some(PgBuiltInType::bool()));
    let encoded = ::serde_json::to_value(any.expression().unwrap())?;
    assert!(encoded["Arith"].get("OpAnyBinary").is_some());
    let equality = select.result_columns().get_column_at(1).unwrap();
    assert_eq!(equality.col_type(), &Some(PgBuiltInType::bool()));
    assert!(
        equality
            .expression()
            .unwrap()
            .operator_binding()
            .unwrap()
            .resolution()
            .is_resolved()
    );
    let encoded = ::serde_json::to_value(equality.expression().unwrap())?;
    let call = &encoded["Arith"]["OpBinary"];
    let oid_array = ::serde_json::to_value(
        ::sql_fun_sqlast::sem::TypeReference::from_master_type_name("_oid"),
    )?;
    assert_eq!(call["left_arg_type"], oid_array);
    assert_eq!(call["right_arg_type"], oid_array);
    Ok(())
}

mod test_helpers {
    pub(super) mod heavy;
}
