//! `PostgreSQL` vector coercion and builtin polymorphism through SQL analysis.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::{
    context::{CastInfoRead, TypeInfoRead},
    sem::TypeReference,
};

#[rstest::rstest]
#[case::oid_cast("'1 2'::oidvector::oid[]", "_oid")]
#[case::int2_cast("'1 2'::int2vector::smallint[]", "_int2")]
#[case::widening("'1 2'::int2vector::integer[]", "_int4")]
#[case::assignment("'1 2'::oidvector::integer[]", "_int4")]
#[case::scalar_unnest("unnest('1 2'::int2vector)", "int2")]
#[case::oid_unnest("unnest('1 2'::oidvector)", "oid")]
#[case::common_append("array_append('1 2'::int2vector, 3::smallint)", "_int2")]
#[case::common_cat("array_cat('1 2'::int2vector, ARRAY[3,4]::smallint[])", "_int2")]
#[case::simple_identity("array_agg('1 2'::int2vector)", "int2vector")]
#[case::vector_array("unnest(ARRAY['1 2'::int2vector])", "int2vector")]
#[case::typed_parameter("unnest($1::int2vector)", "int2")]
fn builtin_vector_results(
    #[case] expression: &str,
    #[case] expected: &str,
) -> testresult::TestResult {
    // The shipped builtin signatures and casts are the contract under test.
    let analyzed = heavy::analyze(&format!("SELECT {expression} AS result"), &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(
        column.col_type(),
        &Some(TypeReference::from_master_type_name(expected))
    );
    let expression = column.expression().unwrap();
    if let Some(function) = expression.as_function_call() {
        assert!(function.overload().is_some());
        assert!(!function.args().is_empty());
        for argument in function.args() {
            assert_eq!(
                argument.arg_expr().get_type().as_ref(),
                argument.arg_definition().as_ref().unwrap().get_type()
            );
        }
    } else {
        let retained = ::serde_json::to_value(expression)?;
        assert!(
            retained["TypeCast"]["resolution"].is_object()
                || retained["Constrained"]["semantic_call"].is_object()
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::int2("int2vector", "int2")]
#[case::oid("oidvector", "oid")]
fn vector_catalog_reads_and_conversion_contexts(
    #[case] vector: &str,
    #[case] element: &str,
) -> testresult::TestResult {
    // Verify JSON catalog loading and frozen schema-layer delegation together.
    let analyzed = heavy::analyze("SELECT 1", &[])?;
    let context = analyzed.context();
    let vector = TypeReference::from_master_type_name(vector);
    let element = TypeReference::from_master_type_name(element);
    let array = element.array_type();
    assert_eq!(context.get_array_item_type(&vector), Some(element));
    assert_eq!(
        context.get_array_item_type(&vector.array_type()),
        Some(vector.clone())
    );
    assert_eq!(vector.is_array(), Some(false));
    let cast = context.get_implicit_cast(&vector, &array).unwrap();
    assert!(!cast.is_no_conversion());
    assert!(context.get_assignment_cast(&vector, &array).is_some());
    assert!(context.get_explicit_cast(&vector, &array).is_some());
    assert!(context.get_explicit_cast(&array, &vector).is_none());
    assert!(context.get_implicit_castable(&vector).contains(&array));
    assert!(context.cast_sources(&array, false).contains(&vector));
    assert!(context.cast_sources(&array, true).contains(&vector));
    let restored: TypeReference = ::serde_json::from_value(::serde_json::to_value(&vector)?)?;
    assert_eq!(restored, vector);
    assert_eq!(
        context.get_array_item_type(&TypeReference::from_full_name(
            ::sql_fun_sqlast::sem::FullName::with_schema("public", "int2vector")
        )),
        None
    );
    Ok(())
}

#[rstest::rstest]
#[case::widening("int2vector", "_int8", true, true)]
#[case::assignment("oidvector", "_int4", false, true)]
#[case::io_explicit("int2vector", "_text", false, false)]
fn vector_element_cast_context_is_preserved(
    #[case] source: &str,
    #[case] target: &str,
    #[case] implicit: bool,
    #[case] assignment: bool,
) -> testresult::TestResult {
    // Checks the actual builtin scalar casts from which vector conversions derive.
    let analyzed = heavy::analyze("SELECT 1", &[])?;
    let context = analyzed.context();
    let source = TypeReference::from_master_type_name(source);
    let target = TypeReference::from_master_type_name(target);
    assert!(context.get_explicit_cast(&source, &target).is_some());
    assert_eq!(
        context.get_implicit_cast(&source, &target).is_some(),
        implicit
    );
    assert_eq!(
        context.get_assignment_cast(&source, &target).is_some(),
        assignment
    );
    assert_eq!(
        context.get_implicit_castable(&source).contains(&target),
        implicit
    );
    assert_eq!(
        context.cast_sources(&target, false).contains(&source),
        implicit
    );
    Ok(())
}

#[test]
fn vector_from_functions_retain_output_bindings() -> testresult::TestResult {
    let definition = "CREATE FUNCTION information_schema._pg_expandarray(anyarray, OUT x anyelement, OUT n integer) RETURNS SETOF record LANGUAGE sql AS ''";
    // Uses the exact shipped SQL declaration plus builtin unnest metadata.
    let analyzed = heavy::analyze(
        "SELECT u.v, e.x, e.n FROM unnest('1 2'::int2vector) AS u(v), information_schema._pg_expandarray('1 2'::int2vector) AS e",
        &[definition],
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    for (index, expected) in [(0, "int2"), (1, "int2"), (2, "int4")] {
        let column = query.result_columns().get_column_at(index).unwrap();
        assert_eq!(
            column.col_type(),
            &Some(TypeReference::from_master_type_name(expected))
        );
        let retained = ::serde_json::to_value(column.expression().unwrap())?;
        let binding = &retained["ColumnRef"]["TableFuncColumn"];
        assert!(binding["table_function"].is_object());
        assert!(binding["column_def"].is_object());
        assert_eq!(binding["args"].as_array().unwrap().len(), 1);
    }
    Ok(())
}

#[rstest::rstest]
#[case::int2("int2vector")]
#[case::oid("oidvector")]
fn vector_operator_binding_keeps_named_inputs(#[case] vector: &str) -> testresult::TestResult {
    // The ordinary-array operator fix also consumes shared simple inference.
    let analyzed = heavy::analyze(&format!("SELECT '1 2'::{vector} = '1 2'::{vector}"), &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(
        column.col_type(),
        &Some(TypeReference::from_master_type_name("bool"))
    );
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
    let expected = ::serde_json::to_value(TypeReference::from_master_type_name(vector))?;
    assert_eq!(call["left_arg_type"], expected);
    assert_eq!(call["right_arg_type"], expected);
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
                "18",
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
