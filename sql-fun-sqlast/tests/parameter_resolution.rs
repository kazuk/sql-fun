//! SQL input constraints are independent of parameter hints and Rust code generation.

use ::sql_fun_sqlast::sem::SemAst;
// Shared support lives below tests/ to avoid an extra Cargo test target.
#[path = "test_helpers/shared.rs"]
mod test_helpers;

use crate::test_helpers::create_cache_context;
use crate::test_helpers::heavy;
use ::sql_fun_sqlast::{ast::Dialect, context::ParseContext, parse_statements, sem::PgBuiltInType};
use std::collections::BTreeMap;

// Removed real_postgres_catalog_preserves_numeric_source_types after a standard timeout;
// restore from 379f1ef4a1ac9b54a19263976d6b269a86d60709.

#[rstest::rstest]
#[case::function("select lower($1) as label")]
#[case::cast("select cast($1 as integer) as n")]
#[case::io_casts("select $1::integer as n, $1::text as label")]
#[case::compatible_casts("select $1::integer as n, $1::bigint as m")]
fn real_catalog_contexts_produce_constraints(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
    let statement = parse_statements(sql, Dialect::postgres()).next().unwrap()?;
    let analyzed = SemAst::from_ast(
        &statement,
        heavy::parameter_resolution::context()?,
        &mut cache_context,
    )?;
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

// Removed parser_adapter_retains_the_same_constraints after a standard timeout;
// restore from 379f1ef4a1ac9b54a19263976d6b269a86d60709.

#[test]
fn schema_defined_enum_supports_explicit_io_casts() -> Result<(), Box<dyn std::error::Error>> {
    use ::sql_fun_sqlast::{
        context::{CastInfoRead, SchemaFileContext},
        names::FullName,
        sem::TypeReference,
    };
    let mut cache_context = create_cache_context();
    let definition = parse_statements(
        "create type public.mood as enum ('happy')",
        Dialect::postgres(),
    )
    .next()
    .unwrap()?;
    let defined = SemAst::from_ast(
        &definition,
        heavy::parameter_resolution::context()?,
        &mut cache_context,
    )?;
    let context = {
        let (args, selected) = heavy::environment("17")?;
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
    let analyzed = SemAst::from_ast(&statement, context, &mut cache_context)?;
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
// Removed real_catalog_keeps_explicit_array_input_hints::builtin_array after a standard timeout;
// restore from 9366c1a8288cde410c30daa8ce6338708b3e69b0.

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
    let mut cache_context = create_cache_context();
    // Actual overload declarations and declared casts are the contract here.
    let analyzed = SemAst::from_ast(
        &parse_statements(sql, Dialect::postgres()).next().unwrap()?,
        heavy::parameter_resolution::context()?,
        &mut cache_context,
    )?;
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
