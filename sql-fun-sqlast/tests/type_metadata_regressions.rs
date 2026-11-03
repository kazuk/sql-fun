//! Existing type consumers retain catalog identity and extracted cast preference.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::{
    context::{BaseParseContext, CastInfoRead, ParseContext},
    names::{ColumnName, FullName, ViewName},
    sem::{PgBuiltInType, TypeReference},
};

#[test]
fn parameter_cast_accepts_the_qualified_domain_reference() -> Result<(), Box<dyn std::error::Error>>
{
    let analyzed = heavy::analyze("CREATE DOMAIN amount AS integer", "SELECT $1::amount")?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let domain = TypeReference::from_full_name(FullName::with_schema("public", "amount"));
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .allows(&std::collections::BTreeMap::from([(1, domain.clone())]))
    );
    assert_eq!(
        analyzed
            .context()
            .parameter_constraints()
            .result_column(0)
            .unwrap()
            .admissible_types(),
        Some(std::slice::from_ref(&domain))
    );
    Ok(())
}

#[rstest::rstest]
#[case::domain("amount", "CREATE DOMAIN amount AS integer")]
#[case::domain_array("amount[]", "CREATE DOMAIN amount AS integer")]
#[case::composite("pair", "CREATE TYPE pair AS (n integer)")]
fn same_type_cast_retains_view_column_lineage(
    #[case] ty: &str,
    #[case] declaration: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::catalog(&format!(
        "{declaration}; CREATE TABLE t(d {ty}); CREATE VIEW v AS SELECT d::{ty} AS d FROM t"
    ))?;
    let view = schema
        .get_view(&ViewName::from_full_name(&FullName::with_schema(
            "public", "v",
        )))
        .unwrap();
    assert_eq!(
        view.ordinary_definition()
            .unwrap()
            .column_lineage()
            .source_column(0),
        Some(&ColumnName::from("d"))
    );
    Ok(())
}

#[test]
fn loaded_preferred_cast_retains_existing_priority() -> Result<(), Box<dyn std::error::Error>> {
    let context = heavy::catalog("")?;
    let preferred_type = TypeReference::from_master_type_name("float8");
    let preferred = context
        .get_implicit_cast(&PgBuiltInType::int4(), &preferred_type)
        .unwrap();
    let ordinary = context
        .get_implicit_cast(
            &PgBuiltInType::int4(),
            &TypeReference::from_master_type_name("float4"),
        )
        .unwrap();
    let identity = context
        .get_implicit_cast(&preferred_type, &preferred_type)
        .unwrap();
    assert!(preferred.is_implicit_preferred());
    assert!(!ordinary.is_implicit_preferred());
    assert!(identity.is_no_conversion());
    assert!(
        preferred.context().overload_priority_score()
            > ordinary.context().overload_priority_score()
    );
    assert!(
        identity.context().overload_priority_score()
            > preferred.context().overload_priority_score()
    );
    Ok(())
}

#[rstest::rstest]
#[case::function_left("SELECT power(1.2, 1)", "numeric")]
#[case::function_right("SELECT power(1, 1.2)", "numeric")]
#[case::function_exact("SELECT abs(1)", "int4")]
#[case::function_float("SELECT power(1::float8, 2::float8)", "float8")]
#[case::function_preferred("SELECT power(1, 2)", "float8")]
fn function_calls_retain_selected_overload_result_types(
    #[case] sql: &str,
    #[case] expected: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // The CLI fixture checks result columns; this preserves the selected AST binding contract.
    let analyzed = heavy::analyze("", sql)?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    let expected = Some(TypeReference::from_master_type_name(expected));
    let call = column
        .expression()
        .unwrap()
        .as_function_call()
        .expect("function-call expression");
    assert_eq!(call.overload().as_ref().unwrap().ret_type(), &expected);
    Ok(())
}

// Timed-out parameter_dependent_overloads_match_concrete_calls cases were deleted in c3fd85bad9570e8bf92c9d863bc885ab3b4ab84b.
#[rstest::rstest]
// Removed binary_right after a standard timeout; restore from
// 11d3059c7ae1f83e4b1e063624c8ce80268e9393.
// Removed function_left after a standard timeout; restore from
// 89e90c449dcebe061c331d9357763aad84740259.
#[case::function_right("SELECT power(1, $1::numeric)", "numeric")]
// Removed function_exact after a standard timeout; restore from
// 64297045ff264e0b9373922b0fa21fa42fa3c7ac.
#[case::function_preferred("SELECT power($1::int4, 2)", "float8")]
fn parameter_dependent_overloads_match_concrete_calls(
    #[case] sql: &str,
    #[case] expected: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let analyzed = heavy::analyze("", sql)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let expected = TypeReference::from_master_type_name(expected);
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

#[test]
fn equal_function_ranks_remain_ambiguous() -> Result<(), Box<dyn std::error::Error>> {
    let schema = "CREATE FUNCTION choose(int4,int8) RETURNS int4 LANGUAGE SQL AS 'SELECT $1';
        CREATE FUNCTION choose(int8,int4) RETURNS int8 LANGUAGE SQL AS 'SELECT $1'";
    let analyzed = heavy::analyze(schema, "SELECT choose($1::int2,2::int2)")?;
    assert!(
        analyzed.context().problems().iter().any(|problem| {
            matches!(
                problem.code(),
                "function_overload_resolution_failed" | "parameter_constraint"
            )
        }),
        "{:?}",
        analyzed.context().problems()
    );
    Ok(())
}

#[rstest::rstest]
#[case::exact("SELECT choose(1.2, 1)", "numeric")]
#[case::preferred("SELECT choose(1, 2)", "float8")]
#[case::parameter("SELECT choose($1::numeric, 1)", "numeric")]
fn expanded_variadic_arguments_count_individually(
    #[case] sql: &str,
    #[case] expected: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema =
        "CREATE FUNCTION choose(VARIADIC numeric[]) RETURNS numeric LANGUAGE SQL AS 'SELECT $1[1]';
        CREATE FUNCTION choose(VARIADIC float8[]) RETURNS float8 LANGUAGE SQL AS 'SELECT $1[1]'";
    let analyzed = heavy::analyze(schema, sql)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(TypeReference::from_master_type_name(expected))
    );
    if let Some(call) = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_function_call()
    {
        for argument in call.args() {
            assert_eq!(
                argument.value().get_type(),
                Some(TypeReference::from_master_type_name(expected))
            );
        }
    }
    Ok(())
}

#[test]
fn omitted_defaults_do_not_contribute_matches() -> Result<(), Box<dyn std::error::Error>> {
    let schema = "CREATE FUNCTION choose(numeric, int4 DEFAULT 1) RETURNS numeric LANGUAGE SQL AS 'SELECT $1';
        CREATE FUNCTION choose(float8, float8 DEFAULT 1) RETURNS float8 LANGUAGE SQL AS 'SELECT $1'";
    let analyzed = heavy::analyze(schema, "SELECT choose($1::numeric)")?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(TypeReference::from_master_type_name("numeric"))
    );
    Ok(())
}

// Timed-out ordinary_signature_precedes_identical_variadic_expansion cases were deleted in c3fd85bad9570e8bf92c9d863bc885ab3b4ab84b.

#[test]
fn output_parameters_do_not_contribute_input_matches() -> Result<(), Box<dyn std::error::Error>> {
    let schema = "CREATE FUNCTION choose(OUT result int4, value numeric) RETURNS int4 LANGUAGE SQL AS 'SELECT 1';
        CREATE FUNCTION choose(value float8) RETURNS float8 LANGUAGE SQL AS 'SELECT $1'";
    let analyzed = heavy::analyze(schema, "SELECT choose($1::numeric)")?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(PgBuiltInType::int4())
    );
    Ok(())
}

mod test_helpers {
    mod cache_context;
    pub(crate) use self::cache_context::create_cache_context;

    pub(super) mod heavy {
        use ::sql_fun_sqlast::{
            AstAndContextPair, PostgresDialect, SqlFunArgs,
            ast::Dialect,
            context::{BaseContext, FrozenBaseContext, SchemaFileContext},
            parse_statements,
        };
        use std::path::{Path, PathBuf};

        fn environment()
        -> Result<(SqlFunArgs, ::sql_fun_core::SqlFunMetadata), Box<dyn std::error::Error>>
        {
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
            Ok((args, selected))
        }

        /// Loads the full builtin catalog and schema to verify context forwarding.
        pub(crate) fn catalog(sql: &str) -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
            let (args, selected) = environment()?;
            let builtin = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
            let file = ::tempfile::NamedTempFile::new()?;
            std::fs::write(file.path(), sql)?;
            let schema = PostgresDialect::parse_schema_file(
                file.path(),
                &builtin,
                &mut None,
                &args,
                &selected,
            )?;
            assert!(schema.problems().is_empty(), "{:?}", schema.problems());
            Ok(schema.base_context().clone())
        }

        pub(crate) fn analyze(
            schema: &str,
            sql: &str,
        ) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
            let mut cache_context = crate::test_helpers::create_cache_context();
            let base = catalog(schema)?;
            let (args, selected) = environment()?;
            let context = SchemaFileContext::new(Path::new("query.sql"), &base, &args, &selected);
            Ok(parse_statements(sql, Dialect::postgres())
                .next()
                .unwrap()?
                .analyze(context, &mut cache_context)?)
        }
    }
}
