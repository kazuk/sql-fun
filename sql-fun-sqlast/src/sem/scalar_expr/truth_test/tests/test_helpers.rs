use crate::{
    StringSpan,
    ast::Dialect,
    context::{BaseParseContext, MutSourcePathWithSpan, SchemaFileContext},
    names::{FullName, SchemaName},
    parse_statements,
    sem::{
        CastContext, CastDefinition, ParseContext, PgBuiltInType, SelectStatement, SemScalarExpr,
        SourcePathWithSpan, TypeCategory, TypeDefinition, TypeReference,
    },
    test_helpers::{TestParseContext, create_cache_context},
};

/// Small type catalog with integer equality and configurable text-to-bool casts.
/// These tests need no builtin catalog loading or external services.
pub(super) fn context(cast: Option<CastContext>) -> SchemaFileContext {
    let mut base = TestParseContext::default().with_dialect(Dialect::postgres());
    base.set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for (ty, category) in [
        (PgBuiltInType::int4(), 'N'),
        (PgBuiltInType::bool(), 'B'),
        (PgBuiltInType::text(), 'S'),
    ] {
        base.setup_type(ty.clone());
        base.set_get_type_impl_result(
            ty.full_name().expect("nominal type identity"),
            &TypeDefinition::builtin_with_category(
                ty.full_name().expect("nominal type identity"),
                Some(TypeCategory::try_from(category).unwrap()),
                None,
            ),
        );
    }
    if let Some(cast) = cast {
        base.set_get_implicit_cast_result(
            &PgBuiltInType::text(),
            &PgBuiltInType::bool(),
            Some(CastDefinition::new(cast)),
        );
    }
    base.setup_binary("=", "int4", "int4", "bool", true);
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    SchemaFileContext::new(
        std::path::Path::new("query.sql"),
        &base.freeze(),
        &args,
        &metadata,
    )
}

pub(super) fn schema(sql: &str) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
    let mut context = context(None);
    context.set_source_path_with_span(&SourcePathWithSpan::new(
        "schema.sql",
        &StringSpan::new(0, sql.len()),
    ));
    for statement in parse_statements(sql, Dialect::postgres()) {
        context = statement?
            .analyze(context, &mut cache_context)?
            .into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(context)
}

pub(super) fn query(
    sql: &str,
    context: SchemaFileContext,
) -> Result<(SelectStatement, SchemaFileContext), Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
    let analyzed = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context, &mut cache_context)?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    Ok((query, analyzed.into_context()))
}

pub(super) fn expression(query: &SelectStatement) -> &SemScalarExpr {
    query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
}

pub(super) fn bool_domain() -> TypeReference {
    TypeReference::from_full_name(FullName::with_schema("public", "flag"))
}
