//! Heterogeneous variadic applicability using the shipped PostgreSQL 18 catalog.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::sem::{FullName, PgBuiltInType, TypeReference};

#[test]
fn shipped_format_keeps_binding_and_name_argument() -> testresult::TestResult {
    // Real imported signatures and their use in builtin views are the contract.
    let analyzed = heavy::analyze_format()?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type(), &Some(PgBuiltInType::text()));
    let call = column.expression().unwrap().as_function_call().unwrap();
    assert!(call.overload().is_some());
    assert_eq!(
        ::serde_json::to_value(call.name_resolution())?["Function"],
        ::serde_json::to_value(FullName::with_schema("pg_catalog", "format"))?
    );
    assert_eq!(call.args().len(), 2);
    let argument = &call.args()[1];
    assert_eq!(
        argument.value().get_type(),
        Some(TypeReference::from_master_type_name("name"))
    );
    assert_eq!(
        argument.arg_definition().as_ref().unwrap().get_type(),
        Some(&TypeReference::from_master_type_name("any"))
    );
    assert_eq!(
        query.result_columns().get_column_at(1).unwrap().col_type(),
        &Some(TypeReference::from_full_name(FullName::with_schema(
            "information_schema",
            "character_data",
        )))
    );
    Ok(())
}

mod test_helpers {
    pub(super) mod heavy {
        use ::sql_fun_sqlast::{
            AstAndContextPair, PostgresDialect, SqlFunArgs,
            ast::Dialect,
            parse_statements,
            sem::{
                BaseContext, FrozenBaseContext, MutSourcePathWithSpan, SchemaFileContext,
                SourcePathWithSpan,
            },
        };
        use std::{collections::BTreeMap, path::PathBuf};

        /// Load PostgreSQL 18 JSON and SQL catalogs for the actual format
        /// declarations and builtin views. Repeated per test process, unshared.
        pub(crate) fn analyze_format()
        -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
            let home = PathBuf::from(env!("SQL_FUN_HOME"));
            let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
            let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
                "sql-fun",
                "--metadata-file",
                metadata.to_str().unwrap(),
                "--sql-fun-home",
                home.to_str().unwrap(),
                "--postgres-version",
                "18",
            ])?;
            let selected = args.load_metadata()?.remove("default").unwrap();
            let base = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
            let schema = PostgresDialect::parse_schema_file(
                &home.join("postgres/18/schema.sql"),
                &base,
                &mut None,
                &args,
                &selected,
            )?;
            let mut counts = BTreeMap::new();
            for problem in schema.problems() {
                *counts.entry(problem.code()).or_insert(0usize) += 1;
            }
            eprintln!(
                "PostgreSQL 18 catalog diagnostics by code: {}",
                ::serde_json::to_string(&counts)?
            );
            let mut context = SchemaFileContext::new(
                std::path::Path::new("format_query.sql"),
                schema.base_context(),
                &args,
                &selected,
            );
            let fixture_sql = "CREATE TABLE e2e_items (id bigint PRIMARY KEY, attname name)";
            context.set_source_path_with_span(&SourcePathWithSpan::new(
                "format_query.sql",
                &::sql_fun_sqlast::StringSpan::new(0, fixture_sql.len()),
            ));
            let analyzed = parse_statements(fixture_sql, Dialect::postgres())
                .next()
                .unwrap()?
                .analyze(context)?;
            Ok(parse_statements(
                r"SELECT format('%s'::text, 'abc'::name),
                          format('%s IS NOT NULL'::text,
                                 COALESCE(at.attname, 'VALUE'::name))
                              ::information_schema.character_data
                   FROM e2e_items AS at",
                Dialect::postgres(),
            )
            .next()
            .unwrap()?
            .analyze(analyzed.into_context())?)
        }
    }
}
