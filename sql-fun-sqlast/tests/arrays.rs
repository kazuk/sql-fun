//! ARRAY semantics through the maintained SQL analysis boundary.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::{
    context::ParseContext,
    sem::{PgBuiltInType, TypeReference},
};

use std::collections::BTreeMap;

#[test]
fn ordered_elements_and_coercions_are_retained() -> testresult::TestResult {
    let analyzed = heavy::analyze("SELECT ARRAY[1,2.5,NULL]", &[])?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    let array = column.expression().unwrap().as_array().unwrap();
    assert_eq!(array.values().len(), 3);
    for value in array.values() {
        assert_eq!(
            value.get_type(),
            Some(TypeReference::from_master_type_name("numeric"))
        );
    }
    let serialized = ::serde_json::to_value(array)?;
    assert!(serialized["values"][0].get("ImplicitCast").is_some());
    assert!(serialized["values"][1].get("Const").is_some());
    assert!(serialized["values"][2].get("ImplicitCast").is_some());
    Ok(())
}

#[rstest::rstest]
#[case::empty("ARRAY[]", "array_empty", 0)]
#[case::categories("ARRAY[1,true]", "array_incompatible_types", 2)]
#[case::typed_string("ARRAY['1'::text,2]", "array_incompatible_types", 2)]
#[case::nested_boundary("ARRAY[ARRAY[NULL],ARRAY[1]]", "array_incompatible_types", 2)]
#[case::explicit_bad("ARRAY[true]::date[]", "array_incompatible_types", 1)]
fn invalid_arrays_keep_failure_and_allow_next_statement(
    #[case] expression: &str,
    #[case] code: &str,
    #[case] count: usize,
) -> testresult::TestResult {
    // The CLI regression asserts diagnostic identities and result metadata;
    // this integration test keeps rendered diagnostics, array shape, and recovery.
    let analyzed = heavy::analyze(&format!("SELECT {expression}"), &[])?;
    let problem = analyzed
        .context()
        .problems()
        .iter()
        .find(|problem| problem.code() == code)
        .expect("array diagnostic");
    assert!(
        problem
            .render_metadata()
            .unwrap()
            .message()
            .contains("ARRAY")
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expression = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    let encoded = ::serde_json::to_value(expression)?;
    let array = encoded
        .get("Array")
        .unwrap_or(&encoded["TypeCast"]["arg"]["Array"]);
    assert_eq!(array["values"].as_array().unwrap().len(), count);
    assert_eq!(array["types"], ::serde_json::json!([]));
    let context = analyzed.into_context();
    let next = ::sql_fun_sqlast::parse_statements(
        "SELECT 1 AS n",
        ::sql_fun_sqlast::ast::Dialect::postgres(),
    )
    .next()
    .unwrap()?
    .analyze(context)?;
    assert_eq!(
        next.ast()
            .clone()
            .into_select_statemtnt()
            .unwrap()
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type(),
        &Some(PgBuiltInType::int4())
    );
    Ok(())
}

// Removed repeated and nested numeric-source cases in 81568731f391becc60f90cbeac057ddd96c0baa6, and the CTE case in 22cf48f9b8718c8f344f40158c272d052a844340.
#[rstest::rstest]
#[case::mixed("SELECT ARRAY[$1,1] AS result")]
#[case::reverse("SELECT ARRAY[1,$1] AS result")]
#[case::subquery("SELECT ARRAY[(SELECT $1),1] AS result")]
fn parameters_retain_correlated_numeric_sources(#[case] sql: &str) -> testresult::TestResult {
    let analyzed = heavy::analyze(sql, &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int8())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert!(!constraints.result_column(0).unwrap().is_any());
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_array()
            .is_some()
    );
    Ok(())
}

// Removed the array typed-assignment variants; restore from 22cf48f9b8718c8f344f40158c272d052a844340.
#[rstest::rstest]
#[case::greatest("GREATEST($1,NULL)")]
#[case::least("LEAST($1,NULL)")]
#[case::case("CASE WHEN true THEN $1 ELSE NULL END")]
fn unanchored_parameters_accept_typed_assignments(
    #[case] expression: &str,
    #[values(false, true)] reverse: bool,
) -> testresult::TestResult {
    // Real builtin casts and comparison facts determine the admissible sources.
    let sql = if reverse {
        format!("SELECT $1::integer, {expression}")
    } else {
        format!("SELECT {expression}, $1::integer")
    };
    let analyzed = heavy::analyze(&sql, &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    let result = constraints.result_column(usize::from(reverse)).unwrap();
    let expected = PgBuiltInType::int4();
    assert!(result.allows(&expected), "{result:?}");
    Ok(())
}

#[rstest::rstest]
#[case::domain("ARRAY[NULL::amount,NULL::amount]")]
#[case::domain_base("ARRAY[NULL::amount,1]")]
#[case::domain_unknown("ARRAY[NULL::amount,NULL]")]
#[case::array_domain("ARRAY[NULL::amount_array,NULL::integer[]]")]
#[case::array_of_domains("ARRAY[NULL::amount[],NULL::integer[]]")]
#[case::enum_values("ARRAY['low'::mood,'high'::mood]")]
#[case::composite("ARRAY[NULL::pair,NULL::pair]")]
#[case::nullable_column("ARRAY[n,NULL] FROM items")]
fn schema_types_and_bindings_survive_frozen_contexts(
    #[case] expression: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(
        &format!("SELECT {expression}"),
        &[
            "CREATE DOMAIN amount AS integer",
            "CREATE DOMAIN amount_array AS integer[]",
            "CREATE TYPE mood AS ENUM ('low','high')",
            "CREATE TYPE pair AS (n integer)",
            "CREATE TABLE items(n integer)",
        ],
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(
        column
            .expression()
            .unwrap()
            .as_array()
            .unwrap()
            .values()
            .len(),
        2
    );
    Ok(())
}

#[test]
fn operands_keep_successful_and_failed_column_bindings() -> testresult::TestResult {
    let analyzed = heavy::analyze(
        "SELECT ARRAY[1,t.n,missing.n] FROM t",
        &["CREATE TABLE t(n integer)"],
    )?;
    assert!(!analyzed.context().problems().is_empty());
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let array = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(array.values().len(), 3);
    assert!(array.values()[1].table_column_reference().is_some());
    let binding = ::serde_json::to_value(&array.values()[2])?;
    assert!(binding["ColumnRef"]["UndefinedAlias"].is_object());
    Ok(())
}

#[rstest::rstest]
#[case::single("ARRAY[pg_control_system()]", 1)]
#[case::repeated("ARRAY[pg_control_system(),pg_control_system()]", 2)]
#[case::nested("ARRAY[ARRAY[pg_control_system()],ARRAY[pg_control_system()]]", 2)]
fn anonymous_record_arrays_keep_their_element_fields(
    #[case] expression: &str,
    #[case] count: usize,
) -> testresult::TestResult {
    // The CLI contract records the absent public type; this checks the internal
    // dynamic array and retains the anonymous record's field definition.
    use ::sql_fun_sqlast::context::TypeInfoRead;
    let analyzed = heavy::analyze(&format!("SELECT {expression}"), &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    let ty = column.col_type().as_ref().unwrap();
    assert!(ty.is_dynamic());
    assert_eq!(ty.is_array(), Some(true));
    let element = analyzed.context().get_array_item_type(ty).unwrap();
    assert!(element.is_record());
    assert_eq!(
        element
            .type_definition()
            .unwrap()
            .get_member_type(analyzed.context(), "pg_control_version"),
        Some(PgBuiltInType::int4())
    );
    let array = column.expression().unwrap().as_array().unwrap();
    assert_eq!(array.values().len(), count);
    let encoded = ::serde_json::to_value(array)?;
    assert!(!encoded.is_null());
    Ok(())
}

#[test]
fn parameter_with_anonymous_records_uses_the_record_family() -> testresult::TestResult {
    let analyzed = heavy::analyze(
        "SELECT ARRAY[$1,pg_control_system(),pg_control_checkpoint()]",
        &[],
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    // The CLI regression checks the result type; this test keeps parameter
    // family admissibility inside the constraint graph.
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(
                1,
                TypeReference::from_master_type_name("record")
            )]))
    );
    Ok(())
}

#[test]
fn public_array_parameter_domains_do_not_select_an_untyped_prepare_signature()
-> testresult::TestResult {
    let analyzed = heavy::analyze("SELECT ARRAY[$1,1]", &[])?;
    assert!(analyzed.context().problems().is_empty());
    let constraints = analyzed.context().parameter_constraints();
    for ty in [PgBuiltInType::int4(), PgBuiltInType::int8()] {
        assert!(constraints.parameter(1).unwrap().constraint().allows(&ty));
        assert!(constraints.allows(&BTreeMap::from([(1, ty.clone())])));
        assert!(
            constraints
                .result_column(0)
                .unwrap()
                .allows(&ty.array_type())
        );
    }
    // The CLI fixture checks absent and explicitly narrowed result types.
    Ok(())
}

#[rstest::rstest]
#[case::single("ARRAY[$1]")]
#[case::null("ARRAY[$1,NULL]")]
#[case::string("ARRAY[$1,'1']")]
#[case::repeated("ARRAY[$1,$1]")]
#[case::nested("ARRAY[ARRAY[$1],ARRAY[$1,NULL]]")]
#[case::subquery("ARRAY[(SELECT $1)]")]
fn unanchored_arrays_keep_owned_constraints(#[case] expression: &str) -> testresult::TestResult {
    // The builtin catalog supplies scalar and array conversion families. The
    // cloned constraint graph must remain inspectable after its context is gone.
    let constraints = {
        let analyzed = heavy::analyze(&format!("SELECT {expression}"), &[])?;
        assert!(
            analyzed.context().problems().is_empty(),
            "{:?}",
            analyzed.context().problems()
        );
        analyzed.context().parameter_constraints().clone()
    };
    assert!(constraints.result_column(0).unwrap().is_any());
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::int8(),
        PgBuiltInType::text(),
    ] {
        assert!(constraints.allows(&BTreeMap::from([(1, ty)])));
    }
    Ok(())
}

#[rstest::rstest]
#[case::direct("SELECT ARRAY[$1,$2]")]
#[case::nested("SELECT ARRAY[ARRAY[$1],ARRAY[$2]]")]
#[case::cte("WITH c AS (SELECT ARRAY[$1,$2] AS n) SELECT n FROM c")]
fn unanchored_array_inputs_remain_jointly_constrained(#[case] sql: &str) -> testresult::TestResult {
    let analyzed = heavy::analyze(sql, &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::int8())
    ])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(!constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::text())
    ])));
    assert!(constraints.allows(&BTreeMap::from([
        (1, TypeReference::from_master_type_name("json")),
        (2, TypeReference::from_master_type_name("json"))
    ])));
    Ok(())
}

#[rstest::rstest]
#[case::predicate("SELECT ARRAY[$1,NULL] WHERE $1", "_bool")]
#[case::cte(
    "WITH c AS (SELECT ARRAY[$1,NULL] AS n) SELECT n FROM c WHERE $1",
    "_bool"
)]
#[case::subscript("SELECT (ARRAY[$1])[1] + 1", "int4")]
fn late_constraints_reach_array_results(
    #[case] sql: &str,
    #[case] ty: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(sql, &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .result_column(0)
            .unwrap()
            .allows(&TypeReference::from_master_type_name(ty))
    );
    Ok(())
}

#[test]
fn deferred_arrays_accept_catalog_defined_types() -> testresult::TestResult {
    // Enum/domain declarations and their catalog cast rules are part of this contract.
    let analyzed = heavy::analyze(
        "SELECT ARRAY[$1,$2]",
        &[
            "CREATE TYPE mood AS ENUM ('low','high')",
            "CREATE TYPE other_mood AS ENUM ('low','high')",
            "CREATE DOMAIN amount AS integer",
        ],
    )?;
    let constraints = analyzed.context().parameter_constraints().clone();
    drop(analyzed);
    let mood = TypeReference::from_full_name(::sql_fun_sqlast::sem::FullName::with_schema(
        "public", "mood",
    ));
    let other = TypeReference::from_full_name(::sql_fun_sqlast::sem::FullName::with_schema(
        "public",
        "other_mood",
    ));
    let amount = TypeReference::from_full_name(::sql_fun_sqlast::sem::FullName::with_schema(
        "public", "amount",
    ));
    assert!(constraints.allows(&BTreeMap::from([(1, mood.clone()), (2, mood.clone())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, mood), (2, other)])));
    assert!(constraints.allows(&BTreeMap::from([(1, amount), (2, PgBuiltInType::int4())])));
    Ok(())
}

#[rstest::rstest]
#[case::constructor("SELECT ARRAY[1::oid] = ARRAY[1::oid]", &[], "_oid")]
#[case::catalog_shape(
    "SELECT polroles = '{0}'::oid[] FROM policies",
    &["CREATE TABLE policies (polroles oid[])"],
    "_oid"
)]
#[case::integers("SELECT ARRAY[1] = ARRAY[1]", &[], "_int4")]
#[case::empty("SELECT ARRAY[]::oid[] = ARRAY[]::oid[]", &[], "_oid")]
#[case::null_array("SELECT NULL::oid[] = ARRAY[1::oid]", &[], "_oid")]
fn ordinary_array_equality_retains_binding_and_types(
    #[case] sql: &str,
    #[case] definitions: &[&str],
    #[case] array_type: &str,
) -> testresult::TestResult {
    // The CLI regression checks the result column; this keeps the resolved
    // operator binding and its argument, result, and strictness metadata.
    let analyzed = heavy::analyze(sql, definitions)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = select.result_columns().get_column_at(0).unwrap();
    let expression = column.expression().unwrap();
    assert!(
        expression
            .operator_binding()
            .unwrap()
            .resolution()
            .is_resolved()
    );
    let encoded = ::serde_json::to_value(expression)?;
    let call = &encoded["Arith"]["OpBinary"];
    let expected = ::serde_json::to_value(TypeReference::from_master_type_name(array_type))?;
    assert_eq!(call["left_arg_type"], expected);
    assert_eq!(call["right_arg_type"], expected);
    assert_eq!(
        call["return_type"],
        ::serde_json::to_value(PgBuiltInType::bool())?
    );
    assert_eq!(call["strict"], true);
    Ok(())
}

#[rstest::rstest]
#[case::different_elements("ARRAY[1::oid] = ARRAY['x'::text]")]
#[case::numeric_elements("ARRAY[1::integer] = ARRAY[1::bigint]")]
#[case::scalar_right("ARRAY[1::oid] = 1::oid")]
#[case::scalar_left("1::oid = ARRAY[1::oid]")]
fn invalid_array_equality_retains_failure(#[case] expression: &str) -> testresult::TestResult {
    // The CLI regression checks the diagnostic set and unresolved result type;
    // this retains the unresolved operator node and its operand payloads.
    let analyzed = heavy::analyze(&format!("SELECT {expression}"), &[])?;
    let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = select.result_columns().get_column_at(0).unwrap();
    let expression = column.expression().unwrap();
    assert!(
        expression
            .operator_binding()
            .unwrap()
            .resolution()
            .is_resolved()
    );
    let encoded = ::serde_json::to_value(expression)?;
    let call = &encoded["Arith"]["UndefinedOperator"];
    assert!(!call["larg"].is_null());
    assert!(!call["rarg"].is_null());
    Ok(())
}

mod test_helpers {
    pub(super) mod heavy {
        use ::sql_fun_sqlast::{
            AstAndContextPair, PostgresDialect, SqlFunArgs,
            ast::Dialect,
            parse_statements,
            sem::{BaseContext, FrozenBaseContext, SchemaFileContext},
        };
        use std::path::{Path, PathBuf};

        /// Loads shipped builtin JSON metadata and optional SQL definitions per call.
        /// Initialization repeats for each test process; no cache is shared.
        pub(crate) fn analyze(
            sql: &str,
            definitions: &[&str],
        ) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
            let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
            let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
                "sqlfun",
                "--metadata-file",
                metadata.to_str().unwrap(),
                "--sql-fun-home",
                env!("SQL_FUN_HOME"),
                "--postgres-version",
                "17",
            ])?;
            let selected = args.load_metadata()?.remove("default").unwrap();
            let mut base = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
            if !definitions.is_empty() {
                let file = ::tempfile::NamedTempFile::new()?;
                std::fs::write(file.path(), definitions.join(";"))?;
                let schema = PostgresDialect::parse_schema_file(
                    file.path(),
                    &base,
                    &mut None,
                    &args,
                    &selected,
                )?;
                assert!(schema.problems().is_empty(), "{:?}", schema.problems());
                base = schema.base_context().clone();
            }
            let context = SchemaFileContext::new(Path::new("query.sql"), &base, &args, &selected);
            Ok(parse_statements(sql, Dialect::postgres())
                .next()
                .unwrap()?
                .analyze(context)?)
        }
    }
}
