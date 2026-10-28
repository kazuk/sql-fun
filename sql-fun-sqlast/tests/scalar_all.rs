//! Scalar ALL through `PostgreSQL` 18's shipped operator and cast metadata.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::sem::PgBuiltInType;

#[rstest::rstest]
#[case::char_type(r#"'r'::"char" <> ALL (ARRAY['t'::"char", 'x'::"char"])"#)]
#[case::name_type("'public'::name <> ALL (ARRAY['pg_catalog'::name, 'information_schema'::name])")]
#[case::false_with_null("1 = ALL (ARRAY[2,NULL]::integer[])")]
#[case::integers("1 = ALL (ARRAY[1,2])")]
#[case::mixed("2::bigint = ALL (ARRAY[1,2]::integer[])")]
#[case::coerced_array("1::numeric = ALL (ARRAY[1,2]::real[])")]
#[case::empty("1 = ALL (ARRAY[]::integer[])")]
#[case::null_left_empty("NULL::integer = ALL (ARRAY[]::integer[])")]
#[case::null_elements("1 = ALL (ARRAY[NULL]::integer[])")]
#[case::null_array("1 = ALL (NULL::integer[])")]
#[case::multidimensional("1 = ALL (ARRAY[[1,2],[3,4]])")]
#[case::int2vector("1::smallint = ALL ('1 1'::int2vector)")]
#[case::oidvector("1::oid = ALL ('1 1'::oidvector)")]
#[case::records("pg_control_system() = ALL (ARRAY[pg_control_system()])")]
fn typed_all_retains_boolean_and_binding(#[case] expression: &str) -> testresult::TestResult {
    // Actual builtin overload and cast families are the contract under test.
    let analyzed = heavy::analyze(&format!("SELECT {expression}"))?;
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
    let call = &encoded["Arith"]["OpAllBinary"];
    assert_eq!(
        call["operator_def"]["result_type"],
        ::serde_json::to_value(PgBuiltInType::bool())?
    );
    assert!(!call["right_arg"].is_null());
    assert!(!call["left_arg"].is_null());
    Ok(())
}

#[rstest::rstest]
#[case::non_array("1 = ALL (1)", "require_array")]
#[case::incompatible(
    "1 = ALL (ARRAY['x']::text[])",
    "binary_operator_overload_resolution_failed"
)]
#[case::array_left(
    "ARRAY[1] = ALL (ARRAY[[1]])",
    "binary_operator_overload_resolution_failed"
)]
#[case::not_boolean("1 + ALL (ARRAY[1,2])", "undefined_operator")]
#[case::missing_operator("1 OPERATOR(public.===) ALL (ARRAY[1,2])", "undefined_operator")]
fn invalid_all_keeps_operands_and_failure(
    #[case] expression: &str,
    #[case] code: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(&format!("SELECT {expression}"))?;
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
    assert_eq!(diagnostic["UndefinedOperator"][0], "AexprOpAll");
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = select.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type(), &None);
    let value = column.expression().unwrap();
    assert_eq!(
        value.operator_binding().unwrap().resolution().is_resolved(),
        !expression.contains("public.===")
    );
    let encoded = ::serde_json::to_value(value)?;
    let call = &encoded["Arith"]["UndefinedOperatorAll"];
    assert!(!call["larg"].is_null());
    assert!(!call["rarg"].is_null());
    Ok(())
}

// Removed shipped_catalog_all_clauses_resolve; restore from 81568731f391becc60f90cbeac057ddd96c0baa6.

mod test_helpers {
    pub(super) mod heavy;
}
