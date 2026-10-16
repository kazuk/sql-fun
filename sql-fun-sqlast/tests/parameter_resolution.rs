//! SQL input constraints are independent of parameter hints and Rust code generation.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::{ast::Dialect, parse_statements, sem::PgBuiltInType};
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
    let constraints = analyzed.parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int2())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int8())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert!(!constraints.parameter(1).unwrap().constraint().is_any());
    assert!(!constraints.result_column(0).unwrap().is_any());
    Ok(())
}

mod test_helpers {
    pub(super) mod heavy {
        use ::sql_fun_sqlast::{
            SqlFunArgs,
            sem::{BaseContext, FrozenBaseContext, SchemaFileContext},
        };
        use std::path::PathBuf;

        /// Loads the full builtin function/operator/cast metadata once per call.
        /// This is intentionally heavy to validate the real catalog contracts.
        pub(crate) fn context() -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
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
            let base = BaseContext::new(&args, args.load_metadata()?.get("default").unwrap())?;
            Ok(SchemaFileContext::new(
                &metadata,
                &FrozenBaseContext::from(base),
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
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::int2())]))
    );
    assert!(
        !analyzed
            .parameter_constraints()
            .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
    );
    Ok(())
}
