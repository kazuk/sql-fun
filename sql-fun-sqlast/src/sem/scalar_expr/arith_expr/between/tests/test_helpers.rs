use crate::context::BaseParseContext;
use crate::{
    ast::Dialect,
    context::MutSourcePathWithSpan,
    sem::{
        BinaryOperatorDefinition, OperatorDefinition, OperatorName, ParseContext, PgBuiltInType,
        SchemaFileContext, SchemaName, SourcePathWithSpan, TypeCategory, TypeDefinition,
    },
    test_helpers::TestParseContext,
};

/// Small catalog with independently selected integer/text comparisons.
/// These tests need no builtin catalog loading or external services.
pub(super) fn context(operators: &[&str]) -> SchemaFileContext {
    comparison_context(operators, false)
}

pub(super) fn mixed_bounds_context(operators: &[&str]) -> SchemaFileContext {
    comparison_context(operators, true)
}

fn comparison_context(operators: &[&str], mixed_bounds: bool) -> SchemaFileContext {
    let mut base = TestParseContext::default().with_dialect(Dialect::postgres());
    base.set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for (ty, category) in [
        (PgBuiltInType::int4(), 'N'),
        (PgBuiltInType::bool(), 'B'),
        (PgBuiltInType::text(), 'S'),
    ] {
        base.setup_type(ty.clone());
        base.set_get_type_impl_result(
            ty.full_name(),
            &TypeDefinition::builtin_with_category(
                ty.full_name(),
                Some(TypeCategory::try_from(category).unwrap()),
            ),
        );
    }
    for operator in operators {
        let pairs = if mixed_bounds {
            [
                (PgBuiltInType::int4(), PgBuiltInType::bool()),
                (PgBuiltInType::int4(), PgBuiltInType::text()),
            ]
        } else {
            [
                (PgBuiltInType::int4(), PgBuiltInType::int4()),
                (PgBuiltInType::text(), PgBuiltInType::text()),
            ]
        };
        let signatures = pairs.map(|(left, right)| {
            BinaryOperatorDefinition::new(&left, &right, &PgBuiltInType::bool(), true)
        });
        base.set_get_operator_op_result(
            &OperatorName::new(operator),
            true,
            true,
            Some(&OperatorDefinition::Binary(signatures.to_vec())),
        );
    }
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
    let mut context = context(&[">=", "<=", "<", ">"]);
    context.set_source_path_with_span(&SourcePathWithSpan::new(
        "schema.sql",
        &crate::StringSpan::new(0, sql.len()),
    ));
    for statement in crate::parse_statements(sql, Dialect::postgres()) {
        context = statement?.analyze(context)?.into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(context)
}

pub(super) fn query(
    sql: &str,
    context: SchemaFileContext,
) -> Result<(crate::sem::SelectStatement, SchemaFileContext), Box<dyn std::error::Error>> {
    let analyzed = crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context)?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    Ok((query, analyzed.into_context()))
}

pub(super) fn expression(query: &crate::sem::SelectStatement) -> &crate::sem::SemScalarExpr {
    query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
}
