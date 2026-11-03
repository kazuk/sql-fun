use crate::context::SchemaFileContext;
use crate::{
    ast::Dialect,
    names::SchemaName,
    sem::{CreateDomain, PgBuiltInType, TypeDefinition, TypeReference},
    test_helpers::TestParseContext,
};

pub(crate) fn context() -> TestParseContext {
    let mut context = TestParseContext::default().with_dialect(Dialect::postgres());
    context
        .set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for ty in [
        PgBuiltInType::int4(),
        PgBuiltInType::int4().array_type(),
        PgBuiltInType::bool(),
    ] {
        context.setup_type(ty);
    }
    context.setup_binary("=", "int4", "int4", "bool", true);
    context
}

pub(crate) fn domains() -> TestParseContext {
    let mut context = context();
    let amount = "public.amount".parse::<TypeReference>().unwrap();
    let array = "public.amount_array".parse::<TypeReference>().unwrap();
    let chain = "public.amount_array_chain"
        .parse::<TypeReference>()
        .unwrap();
    for (domain, base) in [
        (&amount, PgBuiltInType::int4()),
        (&array, PgBuiltInType::int4().array_type()),
        (&chain, array.clone()),
    ] {
        context.set_get_type_impl_result(
            domain.full_name().expect("nominal type identity"),
            &TypeDefinition::from(&CreateDomain::new(domain, &Some(base))),
        );
    }
    context
}

/// Resolve caller-supplied domain DDL in the finite quantified-comparison catalog.
pub(crate) fn schema(sql: &str) -> testresult::TestResult<SchemaFileContext> {
    use crate::{
        parse_statements,
        test_helpers::{create_cache_context, scalar_catalog::schema_from_base},
    };
    let mut context = schema_from_base(context(), std::path::Path::new("query.sql"));
    let mut cache_context = create_cache_context();
    for statement in parse_statements(sql, Dialect::postgres()) {
        context = statement?
            .analyze(context, &mut cache_context)?
            .into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(context)
}
