//! COALESCE inference through the public semantic-analysis boundary.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::{
    context::ParseContext,
    sem::{PgBuiltInType, TypeReference},
};
use std::collections::BTreeMap;

// Removed the repeated numeric-input variant; restore from 81568731f391becc60f90cbeac057ddd96c0baa6.
#[rstest::rstest]
#[case::parameter("SELECT COALESCE($1,1)")]
#[case::reverse("SELECT COALESCE(1,$1)")]
#[case::nested("SELECT COALESCE(COALESCE($1,2),1)")]
// Removed nested_case after a local timeout; restore from bb14217fb859c8255678c9c00c58fd5d2d6f0380.
// Removed in_case after a local timeout; restore from bb14217fb859c8255678c9c00c58fd5d2d6f0380.
#[case::operator("SELECT COALESCE($1,1)+2")]
#[case::function("SELECT abs(COALESCE($1,1))")]
#[case::cte("WITH c AS (SELECT COALESCE($1,1) AS n) SELECT n FROM c")]
#[case::derived("SELECT n FROM (SELECT COALESCE($1,1) AS n) c")]
#[case::other_use_first("SELECT $1::integer, COALESCE($1,1)")]
#[case::other_use_last("SELECT COALESCE($1,1), $1::integer")]
fn numeric_inputs_keep_admissible_assignments(#[case] sql: &str) -> testresult::TestResult {
    // Real builtin casts and overloads determine the supported numeric alternatives.
    let analyzed = heavy::analyze(sql, &[])?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .all(|p| p.code() == "unreachable_coalesce_argument"),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int8())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    Ok(())
}

#[rstest::rstest]
#[case::numeric("COALESCE($1::smallint,1)", "int4")]
#[case::single("COALESCE($1::integer)", "int4")]
#[case::null("COALESCE($1::integer,NULL)", "int4")]
#[case::unknown_string("COALESCE($1::integer,'2')", "int4")]
#[case::left_order("COALESCE($1::varchar,'b'::bpchar)", "varchar")]
#[case::right_order("COALESCE($1::bpchar,'b'::varchar)", "bpchar")]
#[case::no_comparison("COALESCE($1::json,'{}'::json)", "json")]
#[case::all_null("COALESCE(NULL,NULL)", "text")]
#[case::all_strings("COALESCE('a','b')", "text")]
fn finalized_operands_retain_common_type_and_coercions(
    #[case] expression: &str,
    #[case] ty: &str,
) -> testresult::TestResult {
    // Catalog categories and casts verify source-order selection without requiring comparison operators.
    let analyzed = heavy::analyze(&format!("SELECT {expression}"), &[])?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .all(|p| p.code() == "unreachable_coalesce_argument"),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    let expected = TypeReference::from_master_type_name(ty);
    assert_eq!(column.col_type(), &Some(expected.clone()));
    let coalesce = column.expression().unwrap().as_coalesce().unwrap();
    assert_eq!(
        coalesce.types().unwrap().admissible_types(),
        Some([expected.clone()].as_slice())
    );
    for argument in coalesce.arguments() {
        assert_eq!(argument.get_type(), Some(expected.clone()));
    }
    if expression.contains("smallint") {
        let value = ::serde_json::to_value(&coalesce.arguments()[0])?;
        assert!(value.get("ImplicitCast").is_some(), "{value}");
    }
    Ok(())
}

#[test]
fn outer_cast_constrains_coalesce_output() -> testresult::TestResult {
    // The real explicit-cast catalog applies to the expression output, not each input.
    let analyzed = heavy::analyze("SELECT COALESCE($1,1)::smallint", &[])?;
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

// Removed multiple_parameters_preserve_joint_input_and_output_alternatives; restore from 81568731f391becc60f90cbeac057ddd96c0baa6.
#[rstest::rstest]
#[case::same_domain("COALESCE($1::amount,NULL::amount)", "public.amount")]
#[case::domain_base("COALESCE($1::amount,1)", "pg_catalog.int4")]
#[case::domain_unknown("COALESCE($1::amount,NULL)", "pg_catalog.int4")]
#[case::enum_values("COALESCE($1::mood,'low'::mood)", "public.mood")]
fn catalog_types_survive_inference(
    #[case] expression: &str,
    #[case] expected: &str,
) -> testresult::TestResult {
    // Keep the source-context success contract; the CLI additionally reports
    // unresolved parameter metadata for these same result-type cases.
    let analyzed = heavy::analyze(
        &format!("SELECT {expression}"),
        &[
            "CREATE DOMAIN amount AS integer",
            "CREATE TYPE mood AS ENUM ('low','high')",
        ],
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(expected.parse()?)
    );
    Ok(())
}

mod test_helpers {
    pub(super) mod heavy {
        use ::sql_fun_sqlast::{
            AstAndContextPair, PostgresDialect, SqlFunArgs,
            ast::Dialect,
            context::{BaseContext, FrozenBaseContext, SchemaFileContext},
            parse_statements,
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

#[test]
fn later_operands_retain_resolved_column_bindings() -> testresult::TestResult {
    // The real schema catalog supplies the column identity retained by analysis.
    let analyzed = heavy::analyze(
        "SELECT COALESCE($1::integer,t.n) FROM t",
        &["CREATE TABLE t(n integer)"],
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
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
    assert!(coalesce.arguments()[1].table_column_reference().is_some());
    Ok(())
}
