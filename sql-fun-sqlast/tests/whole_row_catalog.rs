//! Real `PostgreSQL` 18 catalog signatures and the shipped schema loading path.
use ::sql_fun_sqlast::{
    PostgresDialect, SqlFunArgs,
    ast::Dialect,
    parse_statements,
    sem::{BaseContext, FrozenBaseContext, FullName, SchemaFileContext, TypeReference},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[test]
fn shipped_catalog_whole_rows() -> testresult::TestResult {
    // This integration assertion requires the actual builtin signatures and SQL
    // views; small-catalog binding cases live beside query resolution tests.
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
    assert!(!counts.contains_key("query_invalid_star_projection_placement"));

    let context = SchemaFileContext::new(
        Path::new("whole_row_query.sql"),
        schema.base_context(),
        &args,
        &selected,
    );
    let analyzed = parse_statements(
        "SELECT information_schema._pg_truetypid(a.*, t.*) FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_type t ON a.atttypid = t.oid",
        Dialect::postgres(),
    ).next().unwrap()?.analyze(context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expression = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    assert_eq!(
        expression.get_type(),
        Some(TypeReference::from_master_type_name("oid"))
    );
    let call = expression.as_function_call().unwrap();
    assert!(call.overload().is_some());
    for (index, (alias, relation)) in [("a", "pg_attribute"), ("t", "pg_type")]
        .into_iter()
        .enumerate()
    {
        let argument = &call.args()[index];
        let expected =
            TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", relation), false);
        assert_eq!(argument.value().get_type(), Some(expected.clone()));
        assert_eq!(
            argument.arg_definition().as_ref().unwrap().get_type(),
            Some(&expected)
        );
        let row = argument.value().as_whole_row().unwrap();
        assert_eq!(row.alias().unwrap().as_str(), alias);
        assert_eq!(row.levels_up(), Some(0));
        assert_eq!(
            row.relation(),
            Some(&FullName::with_schema("pg_catalog", relation))
        );
        assert!(row.failure_code().is_none());
    }
    Ok(())
}
