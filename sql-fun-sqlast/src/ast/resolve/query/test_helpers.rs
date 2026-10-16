use crate::ast::{Dialect, StatementNode};
use crate::sem::{
    AnalysisError, BinaryOperatorDefinition, CastContext, CastDefinition, ColumnName,
    FunctionParam, MutSourcePathWithSpan, OperatorDefinition, OperatorName, OverloadVariant,
    PgBuiltInType, Resolution, SchemaName, SelectStatement, TypeReference,
};
use crate::test_helpers::{TestParseContext, source_span};

pub(super) fn catalog() -> TestParseContext {
    let mut context = TestParseContext::default();
    context.set_source_path_with_span(&source_span());
    context
        .set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for ty in [
        PgBuiltInType::int2(),
        PgBuiltInType::int4(),
        PgBuiltInType::int8(),
        PgBuiltInType::bool(),
        PgBuiltInType::text(),
        PgBuiltInType::numeric(),
        TypeReference::from_master_type_name("interval"),
        TypeReference::from_master_type_name("date"),
    ] {
        context.setup_type(ty.clone());
        context.set_get_implicit_cast_result(
            &ty,
            &ty,
            Some(CastDefinition::new(CastContext::no_conversion())),
        );
    }
    context.set_get_implicit_cast_result(
        &PgBuiltInType::int4(),
        &PgBuiltInType::int8(),
        Some(CastDefinition::new(CastContext::implicit())),
    );
    context.set_get_implicit_cast_result(
        &PgBuiltInType::int2(),
        &PgBuiltInType::int4(),
        Some(CastDefinition::new(CastContext::implicit())),
    );
    let int = PgBuiltInType::int4();
    let add = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(&int, &int, &int, true)]);
    context.set_get_operator_op_result(
        &OperatorName::from_parts(None, "+"),
        true,
        true,
        Some(&add),
    );
    context.set_get_operator_op_result(
        &OperatorName::from_parts(None, "<"),
        true,
        true,
        Some(&add),
    );
    let sum = OverloadVariant::new(
        &Some(int.clone()),
        &vec![FunctionParam::new_input_param(&None, &Some(int), &None)],
        false,
        false,
    );
    context.setup_function("sum", &[sum]);
    context.setup_function(
        "mode",
        &[OverloadVariant::new(
            &Some(PgBuiltInType::int4()),
            &vec![],
            false,
            false,
        )],
    );
    context
}

pub(super) fn resolve(
    sql: &str,
    dialect: Dialect,
    infer: bool,
) -> Result<(SelectStatement, TestParseContext), AnalysisError> {
    let statement = crate::parse_statements(sql, dialect)
        .next()
        .unwrap()
        .unwrap();
    let StatementNode::Query(query) = statement.node else {
        panic!("query fixture expected");
    };
    let mut catalog = catalog();
    let mut resolution = Resolution::new(&catalog);
    let query = query.resolve_in(&mut resolution, dialect, None);
    let query = if infer {
        resolution.infer_query(query, dialect)
    } else {
        query
    };
    let query = resolution.finish(query).report(&mut catalog)?;
    Ok((query, catalog))
}

pub(super) fn query(sql: &str) -> SelectStatement {
    resolve(sql, Dialect::postgres(), false).unwrap().0
}

pub(super) fn output_index(expression: &crate::sem::SemScalarExpr) -> Option<usize> {
    expression.output_index()
}

pub(super) fn output_type(query: &SelectStatement, name: &str) -> Option<TypeReference> {
    query
        .get_result_column_def(&ColumnName::from(name))
        .unwrap()
        .get_type_cloned()
}
