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

    pub(super) fn serialized_parameter_domains(
        value: &::serde_json::Value,
    ) -> Vec<::sql_fun_sqlast::sem::InputTypeConstraint> {
        let mut domains = Vec::new();
        match value {
            ::serde_json::Value::Object(fields) => {
                if let Some(parameter) = fields.get("Param") {
                    domains.push(
                        ::serde_json::from_value(parameter["constraint"].clone())
                            .expect("serialized input domain"),
                    );
                }
                for child in fields.values() {
                    domains.extend(serialized_parameter_domains(child));
                }
            }
            ::serde_json::Value::Array(values) => {
                for child in values {
                    domains.extend(serialized_parameter_domains(child));
                }
            }
            _ => {}
        }
        domains
    }

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

#[rstest::rstest]
#[case::numeric("select 1 where (select $1 where $1 is null) + 1 > 0")]
#[case::cast("select 1 where (select $1 where $1 is null)::integer > 0")]
fn nested_queries_retain_solved_parameter_domains(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cache_context = crate::test_helpers::create_cache_context();
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(heavy::context()?, &mut cache_context)?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let serialized = ::serde_json::to_value(&query)?;
    let domains = test_helpers::serialized_parameter_domains(&serialized);
    assert_ne!(domains, []);
    for domain in domains {
        assert!(!domain.is_any());
        assert!(domain.allows(&PgBuiltInType::int4()));
        assert_eq!(
            &domain,
            analyzed
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::builtin_array("integer[]", "pg_catalog.int4[]")]
#[case::composite_array("hint_row[]", "public.hint_row[]")]
fn real_catalog_keeps_explicit_array_input_hints(
    #[case] hint: &str,
    #[case] expected: &str,
) -> testresult::TestResult {
    use ::sql_fun_sqlast::context::ParameterTypeHintWrite;
    let mut cache_context = crate::test_helpers::create_cache_context();
    // This verifies the public hint API against actual polymorphic equality and
    // cast metadata; the CLI's named-placeholder lexer accepts identifier hints.
    let context = heavy::context()?;
    let mut context = parse_statements(
        "CREATE TYPE hint_row AS (value integer)",
        Dialect::postgres(),
    )
    .next()
    .unwrap()?
    .analyze(context, &mut cache_context)?
    .into_context();
    context.setup_typehinting(&[Some(hint)])?;
    let sql = format!("SELECT $1 IS DISTINCT FROM NULL::{hint}");
    let analyzed = parse_statements(&sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context, &mut cache_context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    let ty = constraints
        .parameter(1)
        .unwrap()
        .constraint()
        .single_type()
        .unwrap();
    assert_eq!(
        ty,
        expected.parse::<::sql_fun_sqlast::sem::TypeReference>()?
    );
    assert!(constraints.allows(&BTreeMap::from([(1, ty)])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    Ok(())
}

#[rstest::rstest]
#[case::integer("integer[]", "pg_catalog.int4[]")]
#[case::uuid("uuid[]", "pg_catalog.uuid[]")]
fn array_hints_resolve_expression_results_without_catalog_search(
    #[case] hint: &str,
    #[case] expected: &str,
) -> testresult::TestResult {
    use ::sql_fun_sqlast::context::ParameterTypeHintWrite;
    let mut cache_context = crate::test_helpers::create_cache_context();
    // Real equality and array-cast declarations are needed for this public API path.
    let mut context = heavy::context()?;
    context.setup_typehinting(&[Some(hint), Some(hint)])?;
    let analyzed = parse_statements(
        "SELECT ($1)[1] IS DISTINCT FROM ($2)[1] AS result",
        Dialect::postgres(),
    )
    .next()
    .unwrap()?
    .analyze(context, &mut cache_context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    for position in [1, 2] {
        assert_eq!(
            analyzed
                .context()
                .parameter_constraints()
                .parameter(position)
                .unwrap()
                .constraint()
                .single_type()
                .unwrap(),
            expected.parse::<::sql_fun_sqlast::sem::TypeReference>()?
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::datetime("SELECT age($1), $1 IS DISTINCT FROM $2", "pg_catalog.date")]
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
