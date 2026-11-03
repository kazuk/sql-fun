use crate::{
    StringSpan,
    ast::Dialect,
    context::{BaseParseContext, MutSourcePathWithSpan, SchemaFileContext},
    names::{OperatorName, SchemaName},
    parse_statements,
    sem::{
        BinaryOperatorDefinition, OperatorDefinition, ParseContext, PgBuiltInType, SelectStatement,
        SemScalarExpr, SourcePathWithSpan, TypeCategory, TypeDefinition,
    },
    test_helpers::{TestParseContext, create_cache_context},
};

/// Small catalog for distinctness binding and inference tests.
/// These tests need no builtin catalog loading or external services.
pub(super) fn context() -> SchemaFileContext {
    context_for(Dialect::postgres())
}

pub(super) fn context_for(dialect: Dialect) -> SchemaFileContext {
    let mut base = TestParseContext::default().with_dialect(dialect);
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
    let definitions = [
        PgBuiltInType::int4(),
        PgBuiltInType::text(),
        PgBuiltInType::bool(),
    ]
    .map(|ty| BinaryOperatorDefinition::new(&ty, &ty, &PgBuiltInType::bool(), true));
    base.set_get_operator_op_result(
        &OperatorName::from_parts(None, "="),
        true,
        true,
        Some(&OperatorDefinition::binary(&definitions)),
    );
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
    let mut context = context();
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
    let analyzed = parse_statements(sql, context.dialect())
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
