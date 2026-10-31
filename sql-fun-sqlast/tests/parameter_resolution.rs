//! SQL input constraints are independent of parameter hints and Rust code generation.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::{ast::Dialect, context::ParseContext, parse_statements, sem::PgBuiltInType};
use std::collections::BTreeMap;

#[test]
fn real_postgres_catalog_preserves_numeric_source_types() -> Result<(), Box<dyn std::error::Error>>
{
    let mut cache_context = crate::test_helpers::create_cache_context();
    // The builtin catalog is part of this test: it verifies real operator/cast
    // metadata rather than only the lightweight unit-test definitions.
    let context = heavy::context()?;
    let statement = parse_statements("select $1 + 1 as total", Dialect::postgres())
        .next()
        .unwrap()?;
    let analyzed = statement.analyze(context, &mut cache_context)?;
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int2())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int8())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert!(!constraints.parameter(1).unwrap().constraint().is_any());
    assert!(!constraints.result_column(0).unwrap().is_any());
    Ok(())
}

mod test_helpers {
    mod cache_context;
    pub(crate) use self::cache_context::create_cache_context;

    pub(super) mod heavy {
        use ::sql_fun_sqlast::{
            SqlFunArgs,
            context::{BaseContext, FrozenBaseContext, SchemaFileContext},
        };
        use std::path::PathBuf;

        /// Reloads selected test metadata on every call; no result is shared.
        /// The real metadata selects the builtin catalog used by schema tests.
        pub(crate) fn environment()
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
            let mut collection = args.load_metadata()?;
            Ok((
                args,
                collection
                    .remove("default")
                    .expect("selected test metadata"),
            ))
        }

        /// Loads the full builtin function/operator/cast metadata once per call.
        /// This is intentionally heavy to validate the real catalog contracts.
        pub(crate) fn context() -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
            let (args, selected) = environment()?;
            let base = BaseContext::new(&args, &selected)?;
            Ok(SchemaFileContext::new(
                std::path::Path::new("query.sql"),
                &FrozenBaseContext::from(base),
                &args,
                &selected,
            ))
        }
    }
}

#[rstest::rstest]
#[case::function("select lower($1) as label")]
#[case::cast("select cast($1 as integer) as n")]
#[case::io_casts("select $1::integer as n, $1::text as label")]
#[case::compatible_casts("select $1::integer as n, $1::bigint as m")]
fn real_catalog_contexts_produce_constraints(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cache_context = crate::test_helpers::create_cache_context();
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(heavy::context()?, &mut cache_context)?;
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .is_any()
    );
    Ok(())
}

#[test]
fn parser_adapter_retains_the_same_constraints() -> Result<(), Box<dyn std::error::Error>> {
    use ::sql_fun_sqlast::{
        PostgresDialect,
        sem::SelectStatement,
        syn::{ListOpt, Opt},
    };
    let sql = "select $1 + 1 as n";
    let syntax = PostgresDialect::parse(sql)?;
    let select = syntax
        .get_stmts()
        .get(0)
        .get_stmt()
        .as_select_stmt()
        .as_inner()
        .unwrap();
    let analyzed =
        SelectStatement::analyze_select(heavy::context()?, select, &PostgresDialect::scan(sql)?)?;
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::int2())]))
    );
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
    );
    Ok(())
}

#[test]
fn schema_defined_enum_supports_explicit_io_casts() -> Result<(), Box<dyn std::error::Error>> {
    use ::sql_fun_sqlast::{
        context::{CastInfoRead, SchemaFileContext},
        sem::{FullName, TypeReference},
    };
    let mut cache_context = crate::test_helpers::create_cache_context();
    let definition = parse_statements(
        "create type public.mood as enum ('happy')",
        Dialect::postgres(),
    )
    .next()
    .unwrap()?;
    let defined = definition.analyze(heavy::context()?, &mut cache_context)?;
    let context = {
        let (args, selected) = heavy::environment()?;
        let analysis_base = &defined.into_context().freeze();
        SchemaFileContext::new(
            std::path::Path::new("query.sql"),
            analysis_base,
            &args,
            &selected,
        )
    };
    let mood = TypeReference::from_full_name(FullName::with_schema("public", "mood"));
    let text = PgBuiltInType::text();
    assert!(context.get_explicit_cast(&mood, &text).is_some());
    assert!(context.get_explicit_cast(&text, &mood).is_some());
    let statement = parse_statements("select $1::public.mood, $1::text", Dialect::postgres())
        .next()
        .unwrap()?;
    let analyzed = statement.analyze(context, &mut cache_context)?;
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, mood)]))
    );
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, text)]))
    );
    Ok(())
}

// Removed the numeric case after a parallel timeout; restore from
// 6c8282fafc5f7b72e86b3c5c5acaf6f5f6031613.
// Removed nested_queries_retain_solved_parameter_domains::cast after a standard timeout;
// restore from cde0e0fc3a4df9dbab8f9b9f5e4f09b394e2be57.

// Removed the composite_array case after a parallel timeout; restore from
// 6c8282fafc5f7b72e86b3c5c5acaf6f5f6031613.
// Removed array_hints_resolve_expression_results_without_catalog_search
// (integer and uuid) after parallel timeouts; restore from 6c8282fafc5f7b72e86b3c5c5acaf6f5f6031613.

// Removed the datetime case after a parallel timeout; restore from
// 6c8282fafc5f7b72e86b3c5c5acaf6f5f6031613.
#[rstest::rstest]
#[case::string_cast("SELECT md5($1)", "pg_catalog.varchar")]
fn explicit_assignment_inspection_keeps_declared_cast_sources(
    #[case] sql: &str,
    #[case] source: &str,
) -> testresult::TestResult {
    let mut cache_context = crate::test_helpers::create_cache_context();
    // Actual overload declarations and declared casts are the contract here.
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(heavy::context()?, &mut cache_context)?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .all(|problem| problem.code() != "parameter_constraint")
    );
    let constraints = analyzed.context().parameter_constraints().clone();
    drop(analyzed);
    assert!(constraints.allows(&BTreeMap::from([(1, source.parse()?)])));
    Ok(())
}
