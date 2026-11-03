use crate::{
    StringSpan,
    ast::Dialect,
    context::{BaseParseContext, MutSourcePathWithSpan, ParseContext, SchemaFileContext},
    names::{FullName, OperatorName, SchemaName},
    parse_statements,
    sem::{
        BinaryOperatorDefinition, CastContext, CastDefinition, OperatorDefinition, OverloadVariant,
        PgBuiltInType, SelectStatement, SemScalarExpr, SourcePathWithSpan, TypeCategory,
        TypeDefinition, TypeReference,
    },
    test_helpers::{TestParseContext, create_cache_context},
};

/// Small catalog for membership binding and inference tests.
/// These tests need no builtin catalog loading or external services.
pub(crate) fn context() -> SchemaFileContext {
    context_for(Dialect::postgres())
}

fn base_context(dialect: Dialect) -> TestParseContext {
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
    for name in ["=", "<>"] {
        base.set_get_operator_op_result(
            &OperatorName::from_parts(None, name),
            true,
            true,
            Some(&OperatorDefinition::binary(&definitions)),
        );
    }
    base
}

pub(crate) fn context_for(dialect: Dialect) -> SchemaFileContext {
    from_base(base_context(dialect))
}

fn from_base(base: TestParseContext) -> SchemaFileContext {
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    SchemaFileContext::new(
        std::path::Path::new("query.sql"),
        &base.freeze(),
        &args,
        &metadata,
    )
}

pub(crate) fn schema(sql: &str) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    apply_schema(sql, context())
}

fn apply_schema(
    sql: &str,
    mut context: SchemaFileContext,
) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    let mut cache_context = create_cache_context();
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

pub(crate) fn query(
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

pub(crate) fn expression(query: &SelectStatement) -> &SemScalarExpr {
    query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
}

/// Concrete function inputs seed deferred composite-star expansion without a
/// full builtin catalog. The two row widths test the same semantic boundary.
pub(crate) fn composite_schema() -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    let mut base = base_context(Dialect::postgres());
    for name in ["singleton", "pair"] {
        let ty = TypeReference::concrete_type_ref(&FullName::with_schema("public", name), false);
        base.set_get_implicit_cast_result(
            &ty,
            &ty,
            Some(CastDefinition::new(CastContext::no_conversion())),
        );
        base.setup_function(
            &format!("take_{name}"),
            &[OverloadVariant::new(
                &Some(PgBuiltInType::int4()),
                &vec![crate::sem::FunctionParam::new_input_param(
                    &None,
                    &Some(ty),
                    &None,
                )],
                false,
                false,
            )],
        );
    }
    apply_schema(
        "CREATE TYPE singleton AS (n integer); CREATE TYPE pair AS (n integer, m integer)",
        from_base(base),
    )
}
