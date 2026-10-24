//! Scalar ALL through PostgreSQL 18's shipped operator and cast metadata.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::sem::{FullName, PgBuiltInType, ViewInfoRead};

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

#[test]
#[ignore = "#749: builtin scalar return-type loader panics during full catalog loading"]
fn shipped_catalog_all_clauses_resolve() -> testresult::TestResult {
    // Full catalog DDL is needed to check all seven affected view clauses.
    let analyzed = heavy::analyze(
        r#"SELECT 'r'::"char" <> ALL (ARRAY['t'::"char", 'x'::"char"]),
                  'public'::name <> ALL (ARRAY['pg_catalog'::name, 'information_schema'::name])"#,
        true,
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    for name in [
        "information_schema.table_constraints",
        "pg_catalog.pg_stat_user_indexes",
        "pg_catalog.pg_stat_user_tables",
        "pg_catalog.pg_stat_xact_user_tables",
        "pg_catalog.pg_statio_user_indexes",
        "pg_catalog.pg_statio_user_sequences",
        "pg_catalog.pg_statio_user_tables",
    ] {
        let name: FullName = name.parse()?;
        let view = analyzed.context().get_view_impl(&name).unwrap();
        let query = view.ordinary_definition().unwrap().query();
        let encoded = ::serde_json::to_value(query)?;
        assert_eq!(test_helpers::assert_all_bindings(&encoded)?, 1, "{name}");
    }
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    for index in 0..2 {
        let column = select.result_columns().get_column_at(index).unwrap();
        assert_eq!(column.col_type(), &Some(PgBuiltInType::bool()));
        let value = column.expression().unwrap();
        assert!(value.operator_binding().unwrap().resolution().is_resolved());
        let encoded = ::serde_json::to_value(value)?;
        assert!(encoded["Arith"].get("OpAllBinary").is_some());
    }
    Ok(())
}

mod test_helpers {
    pub(super) mod heavy;

    pub(super) fn assert_all_bindings(
        value: &::serde_json::Value,
    ) -> Result<usize, ::serde_json::Error> {
        use ::sql_fun_sqlast::sem::{PgBuiltInType, SemScalarExpr};

        if value["Arith"].get("OpAllBinary").is_some() {
            let expression: SemScalarExpr = ::serde_json::from_value(value.clone())?;
            assert_eq!(expression.get_type(), Some(PgBuiltInType::bool()));
            assert_eq!(expression.is_not_null(), None);
            assert!(
                expression
                    .operator_binding()
                    .unwrap()
                    .resolution()
                    .is_resolved()
            );
            return Ok(1);
        }
        let mut count = 0;
        match value {
            ::serde_json::Value::Object(fields) => {
                for child in fields.values() {
                    count += assert_all_bindings(child)?;
                }
            }
            ::serde_json::Value::Array(values) => {
                for child in values {
                    count += assert_all_bindings(child)?;
                }
            }
            _ => {}
        }
        Ok(count)
    }
}
