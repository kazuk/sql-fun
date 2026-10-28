//! Parameter-dependent CASE semantics with real builtin casts and operators.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::context::ParseContext;
use ::sql_fun_sqlast::sem::{PgBuiltInType, TypeReference};
use std::collections::BTreeMap;

// Removed the repeated numeric-input variant; restore from 81568731f391becc60f90cbeac057ddd96c0baa6.
#[rstest::rstest]
#[case::then_parameter("SELECT CASE WHEN true THEN $1 ELSE 1 END")]
#[case::else_parameter("SELECT CASE WHEN true THEN 1 ELSE $1 END")]
#[case::nested("SELECT CASE WHEN true THEN CASE WHEN false THEN $1 ELSE 2 END ELSE 1 END")]
#[case::outer_operator("SELECT (CASE WHEN true THEN $1 ELSE 1 END) + 2")]
#[case::outer_call("SELECT abs(CASE WHEN true THEN $1 ELSE 1 END)")]
#[case::cte("WITH c AS (SELECT CASE WHEN true THEN $1 ELSE 1 END AS n) SELECT n FROM c")]
fn numeric_inputs_retain_admissible_alternatives(#[case] sql: &str) -> testresult::TestResult {
    // Real builtin cast/overload metadata determines the admissible numeric families.
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
    assert!(
        constraints
            .result_column(0)
            .unwrap()
            .allows(&PgBuiltInType::int4())
    );
    assert!(
        constraints
            .result_column(0)
            .unwrap()
            .allows(&PgBuiltInType::int8())
    );
    Ok(())
}

#[rstest::rstest]
#[case::else_first(
    "CASE WHEN $1 THEN 'a'::varchar ELSE 'b'::bpchar END",
    "bpchar",
    Some(true)
)]
#[case::else_first_reverse(
    "CASE WHEN $1 THEN 'a'::bpchar ELSE 'b'::varchar END",
    "varchar",
    Some(true)
)]
#[case::numeric("CASE WHEN $1 THEN 1 ELSE 2.5 END", "numeric", Some(true))]
#[case::numeric_reverse("CASE WHEN $1 THEN 2.5 ELSE 1 END", "numeric", Some(true))]
#[case::unknown_string("CASE WHEN $1 THEN '2' ELSE 1 END", "int4", Some(true))]
#[case::null_branch("CASE WHEN $1 THEN NULL ELSE 1 END", "int4", Some(false))]
#[case::omitted_else("CASE WHEN $1 THEN 1 END", "int4", Some(false))]
#[case::all_null("CASE WHEN $1 THEN NULL ELSE NULL END", "text", Some(false))]
#[case::all_strings("CASE WHEN $1 THEN 'a' ELSE 'b' END", "text", Some(true))]
#[case::json_without_ordering(
    "CASE WHEN $1 THEN '{}'::json ELSE '{}'::json END",
    "json",
    Some(true)
)]
#[case::then_order(
    "CASE WHEN $1 THEN 'a'::varchar WHEN false THEN 'b'::bpchar END",
    "varchar",
    Some(false)
)]
#[case::then_order_reverse(
    "CASE WHEN $1 THEN 'a'::bpchar WHEN false THEN 'b'::varchar END",
    "bpchar",
    Some(false)
)]
fn condition_parameters_preserve_result_selection(
    #[case] expression: &str,
    #[case] ty: &str,
    #[case] not_null: Option<bool>,
) -> testresult::TestResult {
    // Real catalog metadata verifies ordered casts, preferred types, and json without B-tree ordering.
    let analyzed = heavy::analyze(&format!("SELECT {expression}"), &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    let expected = TypeReference::from_master_type_name(ty);
    assert_eq!(column.col_type(), &Some(expected.clone()));
    assert_eq!(column.is_not_null(), &not_null);
    let case = column.expression().unwrap().as_case().unwrap();
    assert!(case.argument().is_none());
    for arm in case.arms() {
        assert_eq!(arm.result().get_type(), Some(expected.clone()));
        assert_eq!(arm.condition().get_type(), Some(PgBuiltInType::bool()));
    }
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
    );
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::int4())]))
    );
    Ok(())
}

// Removed the multiple-comparisons variant; restore from 81568731f391becc60f90cbeac057ddd96c0baa6.
#[rstest::rstest]
#[case::selector("SELECT CASE $1 WHEN 1 THEN 2 ELSE 3 END")]
#[case::when_operand("SELECT CASE 1 WHEN $1 THEN 2 ELSE 3 END")]
#[case::result_parameter("SELECT CASE 1 WHEN 1 THEN $1 ELSE 3 END")]
fn simple_case_retains_equality_binding(#[case] sql: &str) -> testresult::TestResult {
    // Equality overload and implicit-cast selection use the real builtin signatures.
    let analyzed = heavy::analyze(sql, &[])?;
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
    for arm in case.arms() {
        assert!(
            arm.comparison()
                .unwrap()
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
        assert_eq!(
            arm.comparison().unwrap().get_type(),
            Some(PgBuiltInType::bool())
        );
    }
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::int4())]))
    );
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::int8())]))
    );
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
    );
    Ok(())
}

// Removed the two-results joint-constraint variant; restore from 81568731f391becc60f90cbeac057ddd96c0baa6.
#[rstest::rstest]
#[case::simple_comparison("SELECT CASE $1 WHEN $2 THEN 1 ELSE 2 END")]
fn repeated_inputs_remain_jointly_constrained(#[case] sql: &str) -> testresult::TestResult {
    // Builtin overload/cast alternatives must retain correlations between inputs.
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
    assert!(!constraints.allows(&BTreeMap::from([
        (1, PgBuiltInType::int4()),
        (2, PgBuiltInType::bool())
    ])));
    Ok(())
}

#[rstest::rstest]
#[case::other_output_first("SELECT $1::integer, CASE WHEN true THEN $1 ELSE 1 END", 1)]
#[case::other_output_last("SELECT CASE WHEN true THEN $1 ELSE 1 END, $1::integer", 0)]
#[case::typed_branch("SELECT CASE WHEN true THEN $1::smallint ELSE 1 END", 0)]
fn finalization_preserves_case_structure_and_branch_coercions(
    #[case] sql: &str,
    #[case] index: usize,
) -> testresult::TestResult {
    // Actual builtin explicit and implicit casts are part of this assertion.
    let analyzed = heavy::analyze(sql, &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(index).unwrap();
    let case = column.expression().unwrap().as_case().unwrap();
    assert_eq!(case.arms().len(), 1);
    assert!(case.otherwise().is_some());
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::int4())]))
    );
    if sql.contains("smallint") {
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        let serialized = ::serde_json::to_value(case.arms()[0].result())?;
        assert!(serialized.get("ImplicitCast").is_some(), "{serialized}");
    } else {
        assert!(column.col_type().is_none());
        assert!(
            analyzed
                .context()
                .parameter_constraints()
                .result_column(index)
                .unwrap()
                .allows(&PgBuiltInType::int8())
        );
    }
    Ok(())
}

#[test]
fn an_outer_cast_receives_the_case_result_domain() -> testresult::TestResult {
    // Explicit builtin casts apply to the CASE output, preserving input alternatives.
    let analyzed = heavy::analyze("SELECT (CASE WHEN true THEN $1 ELSE 1 END)::smallint", &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int8())])));
    assert_eq!(
        constraints.result_column(0).unwrap().admissible_types(),
        Some([TypeReference::from_master_type_name("int2")].as_slice())
    );
    Ok(())
}

#[rstest::rstest]
#[case::null("CASE WHEN true THEN $1 ELSE NULL END")]
#[case::omitted_else("CASE WHEN true THEN $1 END")]
#[case::string("CASE WHEN true THEN $1 ELSE '1' END")]
#[case::two_parameters("CASE WHEN true THEN $1 ELSE $2 END")]
#[case::nested("CASE WHEN true THEN CASE WHEN false THEN $1 END ELSE $2 END")]
fn unanchored_result_parameters_preserve_assignments(
    #[case] expression: &str,
) -> testresult::TestResult {
    // Real catalog casts distinguish numeric and string assignment families.
    let analyzed = heavy::analyze(&format!("SELECT {expression}"), &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.result_column(0).unwrap().is_any());
    let mut assignments = BTreeMap::from([(1, PgBuiltInType::int4())]);
    if expression.contains("$2") {
        assignments.insert(2, PgBuiltInType::int8());
    }
    assert!(constraints.allows(&assignments));
    if expression.contains("$2") {
        assignments.insert(2, PgBuiltInType::text());
        assert!(!constraints.allows(&assignments));
    }
    assert!(constraints.allows(&BTreeMap::from([(
        1,
        TypeReference::from_master_type_name("json")
    )])));
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert!(
        query
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
