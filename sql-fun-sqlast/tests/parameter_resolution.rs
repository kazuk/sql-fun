//! SQL input constraints are independent of parameter hints and Rust code generation.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::{
    ast::Dialect,
    parse_statements,
    sem::{ParseContext, PgBuiltInType},
};
use std::collections::BTreeMap;

#[test]
fn real_postgres_catalog_preserves_numeric_source_types() -> Result<(), Box<dyn std::error::Error>>
{
    // The builtin catalog is part of this test: it verifies real operator/cast
    // metadata rather than only the lightweight unit-test definitions.
    let context = heavy::context()?;
    let statement = parse_statements("select $1 + 1 as total", Dialect::postgres())
        .next()
        .unwrap()?;
    let analyzed = statement.analyze(context)?;
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
            sem::{BaseContext, FrozenBaseContext, SchemaFileContext},
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

        /// Reloads and parses the full builtin catalog, then analyzes the supplied
        /// schema on every call to exercise real schema casts and row types.
        /// The frozen path additionally reloads metadata when rebuilding the
        /// context. Initialization is repeated per call and is not shared.
        pub(crate) fn schema(
            sql: &str,
            frozen: bool,
        ) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
            use ::sql_fun_sqlast::{
                StringSpan,
                sem::{MutSourcePathWithSpan, SourcePathWithSpan},
            };
            use ::sql_fun_sqlast::{ast::Dialect, parse_statements, sem::ParseContext};
            let mut context = context()?;
            context.set_source_path_with_span(&SourcePathWithSpan::new(
                "schema.sql",
                &StringSpan::new(0, sql.len()),
            ));
            for definition in parse_statements(sql, Dialect::postgres()) {
                context = definition?.analyze(context)?.into_context();
            }
            if frozen {
                let (args, selected) = environment()?;
                context = SchemaFileContext::new(
                    std::path::Path::new("query.sql"),
                    &context.freeze(),
                    &args,
                    &selected,
                );
            }
            Ok(context)
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
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(heavy::context()?)?;
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
    use ::sql_fun_sqlast::sem::{CastInfoRead, FullName, SchemaFileContext, TypeReference};
    let definition = parse_statements(
        "create type public.mood as enum ('happy')",
        Dialect::postgres(),
    )
    .next()
    .unwrap()?;
    let defined = definition.analyze(heavy::context()?)?;
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
    let analyzed = statement.analyze(context)?;
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
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(heavy::context()?)?;
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
#[case::numeric_index("SELECT (NULL::integer[])[1.0::numeric]")]
#[case::numeric_bound("SELECT (NULL::integer[])[1.0::numeric:2]")]
fn postgres_assignment_subscripts_are_legal(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Real cast metadata determines which subscript conversions PostgreSQL accepts.
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(heavy::context()?)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    Ok(())
}

#[rstest::rstest]
#[case::index("SELECT ($1::integer[])[$2] AS n", false, 0)]
#[case::slice("SELECT ($1::integer[])[$2:$3] AS n", true, 0)]
#[case::upper_only("SELECT ($1::integer[])[:$2] AS n", true, 0)]
#[case::lower_only("SELECT ($1::integer[])[$2:] AS n", true, 0)]
#[case::repeated_base("SELECT ($1)[$2] AS n, $1::integer[]", false, 0)]
#[case::repeated_base_first("SELECT $1::integer[], ($1)[$2] AS n", false, 1)]
fn parameterized_array_access_links_results(
    #[case] sql: &str,
    #[case] array_result: bool,
    #[case] column_index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // The shipped catalog supplies real PostgreSQL casts and array identities.
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(heavy::context()?)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    let expected = ::sql_fun_sqlast::sem::TypeReference::concrete_type_ref(
        PgBuiltInType::int4().full_name(),
        array_result,
    );
    assert!(
        constraints
            .result_column(column_index)
            .unwrap()
            .allows(&expected)
    );
    assert!(!constraints.result_column(column_index).unwrap().is_any());
    assert!(constraints.allows(&BTreeMap::from([(2, PgBuiltInType::numeric())])));
    assert!(!constraints.allows(&BTreeMap::from([(2, PgBuiltInType::bool())])));
    Ok(())
}

#[rstest::rstest]
#[case::index("SELECT ($1)[$2]")]
#[case::slice("SELECT ($1)[$2:$3]")]
fn underconstrained_array_access_preserves_structure(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Real metadata exercises deferred assignment coercions without selecting a base type.
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(heavy::context()?)?;
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.parameter(1).unwrap().constraint().is_any());
    assert!(constraints.result_column(0).unwrap().is_any());
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    let array = ::sql_fun_sqlast::sem::TypeReference::concrete_type_ref(
        PgBuiltInType::int4().full_name(),
        true,
    );
    assert!(constraints.allows(&BTreeMap::from([(1, array)])));
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expression = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    assert!(expression.as_indirection().is_some());
    let domains = test_helpers::serialized_parameter_domains(&::serde_json::to_value(expression)?);
    assert_eq!(domains.len(), constraints.iter().len());
    assert!(domains[0].is_any());
    assert!(!domains[1].is_any());
    assert!(domains[1].allows(&PgBuiltInType::numeric()));
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &None
    );
    Ok(())
}

#[rstest::rstest]
#[case::invalid_index("SELECT ($1::integer[])[$2::boolean]")]
#[case::invalid_slice("SELECT ($1::integer[])[$2::boolean:]")]
#[case::scalar_base("SELECT ($1::integer)[$2]")]
#[case::constant_index("SELECT ($1)[true]")]
fn invalid_parameterized_access_retains_conflicts(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Cast metadata is required to distinguish PostgreSQL-legal source assignments.
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(heavy::context()?)?;
    assert!(!analyzed.context().problems().is_empty());
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::new())
    );
    Ok(())
}

#[rstest::rstest]
#[case::operator("SELECT ($1)[$2] + 1 AS n")]
#[case::cast_result("SELECT (($1)[$2])::integer AS n")]
#[case::slice_then_index("SELECT (($1::integer[])[$2:$3])[$4] AS n")]
fn surrounding_context_constrains_array_access(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Real operators and casts narrow structural relationships through the graph.
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(heavy::context()?)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    let array = ::sql_fun_sqlast::sem::TypeReference::concrete_type_ref(
        PgBuiltInType::int4().full_name(),
        true,
    );
    assert!(constraints.allows(&BTreeMap::from([(1, array)])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(
        constraints
            .result_column(0)
            .unwrap()
            .allows(&PgBuiltInType::int4())
    );
    assert!(!constraints.result_column(0).unwrap().is_any());
    Ok(())
}

#[rstest::rstest]
#[case::forward("select ($1::public.point_value).n", 0)]
#[case::deferred("select ($1).n, $1::public.point_value", 0)]
#[case::reverse_order("select $1::public.point_value, ($1).n", 1)]
#[case::array_element("select (($1::public.point_value[])[1]).n", 0)]
fn composite_access_propagates_named_field_types(
    #[case] sql: &str,
    #[case] result: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    use ::sql_fun_sqlast::sem::{FullName, TypeReference};
    // Real casts are needed to connect repeated uses through explicit SQL casts.
    let definition = parse_statements(
        "create type public.point_value as (n integer)",
        Dialect::postgres(),
    )
    .next()
    .unwrap()?;
    let context = definition.analyze(heavy::context()?)?.into_context();
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    let domain = constraints.result_column(result).unwrap();
    assert!(!domain.is_any());
    assert!(domain.allows(&PgBuiltInType::int4()));
    assert!(!domain.allows(&PgBuiltInType::text()));
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expression = query
        .result_columns()
        .get_column_at(result)
        .unwrap()
        .expression()
        .unwrap();
    assert_eq!(expression.get_type(), Some(PgBuiltInType::int4()));
    let access = expression.as_indirection().expect("retained member access");
    let ::sql_fun_sqlast::sem::IndirectionExpr::MemberRef(member) = access else {
        panic!("member binding");
    };
    assert_eq!(member.member(), "n");
    assert!(member.base().get_type().is_some());
    let base = TypeReference::concrete_type_ref(
        &FullName::with_schema("public", "point_value"),
        sql.contains("[]"),
    );
    assert!(constraints.allows(&BTreeMap::from([(1, base)])));
    Ok(())
}

#[rstest::rstest]
#[case::unbounded("select ($1).n", true)]
#[case::result_constrained("select (($1).n)::integer", false)]
fn deferred_composite_accepts_owned_anonymous_definitions(
    #[case] sql: &str,
    #[case] unbounded_result: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use ::sql_fun_sqlast::sem::{
        ColumnCollection, ColumnDefinition, ColumnName, TypeDefinition, TypeReference,
    };
    // The builtin cast catalog is used by the result-context case.
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(heavy::context()?)?;
    let constraints = analyzed.context().parameter_constraints().clone();
    drop(analyzed);
    assert!(constraints.parameter(1).unwrap().constraint().is_any());
    assert_eq!(
        constraints.result_column(0).unwrap().is_any(),
        unbounded_result
    );
    let record = |name: &str, ty: &TypeReference| {
        TypeReference::dynamic(&TypeDefinition::ImplicitComposite(ColumnCollection::new(
            vec![ColumnDefinition::new(
                &Some(ColumnName::from(name)),
                Some(ty),
                None,
                &[],
            )],
        )))
    };
    assert!(constraints.allows(&BTreeMap::from([(1, record("n", &PgBuiltInType::int4()))])));
    assert!(!constraints.allows(&BTreeMap::from([(
        1,
        record("other", &PgBuiltInType::int4())
    )])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    Ok(())
}

#[rstest::rstest]
#[case::mutable(false)]
#[case::frozen(true)]
fn nested_composite_and_table_fields_use_owned_definitions(
    #[case] frozen: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use ::sql_fun_sqlast::sem::{FullName, TypeReference};
    // Real schema casts and table row types are the contracts under test.
    let context = heavy::schema(
        "create type public.inner_value as (n integer); create type public.outer_value as (v public.inner_value); create table public.table_value (v public.inner_value);",
        frozen,
    )?;
    let statement = parse_statements(
        "select (($1::public.outer_value).v).n, (($2::public.table_value).v).n",
        Dialect::postgres(),
    )
    .next()
    .unwrap()?;
    let analyzed = statement.analyze(context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints().clone();
    drop(analyzed);
    for column in 0..2 {
        let domain = constraints.result_column(column).unwrap();
        assert!(!domain.is_any());
        assert!(domain.allows(&PgBuiltInType::int4()));
    }
    assert!(constraints.allows(&BTreeMap::from([
        (
            1,
            TypeReference::from_full_name(FullName::with_schema("public", "outer_value"))
        ),
        (
            2,
            TypeReference::from_full_name(FullName::with_schema("public", "table_value"))
        ),
    ])));
    Ok(())
}

#[rstest::rstest]
#[case::array_base(
    "select ($1::public.point_value[]).n",
    "member_access_requires_composite_type"
)]
#[case::missing_member(
    "select ($1).missing, $1::public.point_value",
    "composite_type_field_not_found"
)]
#[case::scalar_base("select ($1).n, $1::integer", "member_access_requires_composite_type")]
fn deferred_member_access_retains_conflicts(
    #[case] sql: &str,
    #[case] diagnostic: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = heavy::schema("create type public.point_value as (n integer)", false)?;
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context)?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == diagnostic),
        "{:?}",
        analyzed.context().problems()
    );
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::new())
    );
    Ok(())
}

#[rstest::rstest]
#[case::missing_member(
    "select (($1).v).missing, $1::public.outer_value",
    "composite_type_field_not_found"
)]
#[case::scalar_base(
    "select ((($1).v).n).missing, $1::public.outer_value",
    "member_access_requires_composite_type"
)]
fn nested_deferred_members_report_structured_errors(
    #[case] sql: &str,
    #[case] diagnostic: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Real catalog definitions exercise deferred nested member resolution.
    let context = heavy::schema(
        "create type public.inner_value as (n integer); create type public.outer_value as (v public.inner_value);",
        false,
    )?;
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context)?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == diagnostic),
        "{:?}",
        analyzed.context().problems()
    );
    assert!(
        !analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::new())
    );
    Ok(())
}

#[test]
fn owned_named_composites_preserve_nominal_identity() -> Result<(), Box<dyn std::error::Error>> {
    use ::sql_fun_sqlast::sem::{BaseParseContext, FullName, TypeReference};
    // Schema cast metadata distinguishes named composites with identical fields.
    let context = heavy::schema(
        "create type public.first_value as (n integer); create type public.second_value as (n integer);",
        false,
    )?;
    let first = FullName::with_schema("public", "first_value");
    let second = FullName::with_schema("public", "second_value");
    let owned_first = TypeReference::dynamic(context.get_type(&first).unwrap());
    let owned_second = TypeReference::dynamic(context.get_type(&second).unwrap());
    let analyzed = parse_statements("select ($1).n, $1::public.first_value", Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context)?;
    let constraints = analyzed.context().parameter_constraints().clone();
    drop(analyzed);
    assert!(constraints.allows(&BTreeMap::from([(1, owned_first)])));
    assert!(!constraints.allows(&BTreeMap::from([(1, owned_second)])));
    assert!(!constraints.allows(&BTreeMap::from([(
        1,
        TypeReference::from_full_name(second)
    )])));
    Ok(())
}
