use crate::{
    ast::{Dialect, StatementNode},
    context::ParseContext,
    parse_statements,
    sem::{
        FunctionParam, OverloadVariant, PgBuiltInType, ScalarConstExpr, SemScalarExpr,
        TypeReference,
    },
    test_helpers::{create_cache_context, query::catalog},
};
use crate::{
    sem::{FuncCallExpr, SelectStatement},
    test_helpers::{TestParseContext, query::expression},
};

pub(crate) fn call(query: &SelectStatement) -> FuncCallExpr {
    expression(query)
        .function_call_snapshot()
        .expect("preserved call")
}

/// A small call catalog with defaults, overloaded inputs and array parameters.
pub(crate) fn resolve(sql: &str, infer: bool) -> (SelectStatement, TestParseContext) {
    let mut context = catalog();
    let default = Some(SemScalarExpr::literal(&ScalarConstExpr::new_integer(3)));
    let int = Some(PgBuiltInType::int4());
    let params = vec![
        FunctionParam::new_input_param(&Some("a".to_owned()), &int, &None),
        FunctionParam::new_input_param(&Some("b".to_owned()), &int, &default),
        FunctionParam::new_input_param(&Some("c".to_owned()), &int, &default),
    ];
    context.setup_function("f", &[OverloadVariant::new(&int, &params, true, true)]);
    context.setup_function(
        "v",
        &[OverloadVariant::new(
            &int,
            &vec![FunctionParam::new_variadig_param(
                &Some("xs".to_owned()),
                &Some(PgBuiltInType::int4().array_type()),
            )],
            true,
            false,
        )],
    );
    context.setup_function(
        "arr",
        &[OverloadVariant::new(
            &int,
            &vec![FunctionParam::new_input_param(
                &Some("xs".to_owned()),
                &Some(PgBuiltInType::int4().array_type()),
                &None,
            )],
            false,
            false,
        )],
    );
    let poly = Some(TypeReference::from_master_type_name("anyelement"));
    context.setup_function(
        "poly",
        &[OverloadVariant::new(
            &poly,
            &vec![FunctionParam::new_input_param(
                &Some("value".to_owned()),
                &poly,
                &None,
            )],
            false,
            false,
        )],
    );
    context.setup_function(
        "polyarray",
        &[OverloadVariant::new(
            &poly,
            &vec![FunctionParam::new_variadig_param(
                &Some("xs".to_owned()),
                &Some(TypeReference::from_master_type_name("anyarray")),
            )],
            false,
            false,
        )],
    );
    context.setup_function(
        "polyout",
        &[OverloadVariant::new(
            &Some(TypeReference::from_master_type_name("record")),
            &vec![
                FunctionParam::new_input_param(&Some("value".to_owned()), &poly, &None),
                FunctionParam::new_output_param(&Some("x".to_owned()), &poly),
                FunctionParam::new_output_param(
                    &Some("y".to_owned()),
                    &Some(PgBuiltInType::text()),
                ),
            ],
            false,
            false,
        )],
    );
    context.setup_type(TypeReference::from_master_type_name("bytea"));
    let bytea = Some(TypeReference::from_master_type_name("bytea"));
    context.setup_function(
        "bytes",
        &[OverloadVariant::new(
            &bytea,
            &vec![FunctionParam::new_input_param(
                &Some("value".to_owned()),
                &bytea,
                &None,
            )],
            false,
            false,
        )],
    );
    context.setup_function(
        "same",
        &[OverloadVariant::new(
            &poly,
            &vec![
                FunctionParam::new_input_param(&Some("a".to_owned()), &poly, &None),
                FunctionParam::new_input_param(&Some("b".to_owned()), &poly, &None),
            ],
            false,
            false,
        )],
    );
    context.setup_aggregate(
        "named_sum",
        &[OverloadVariant::new(
            &int,
            &vec![FunctionParam::new_input_param(
                &Some("x".to_owned()),
                &int,
                &None,
            )],
            false,
            false,
        )],
    );
    let statement = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let StatementNode::Query(query) = statement.into_test_node() else {
        panic!("query")
    };
    context.begin_parameter_type_analysis();
    let query = query.resolve_test_in(&mut context, None);
    let query = if infer {
        context.infer_query_parameter_types(query, &mut create_cache_context())
    } else {
        query
    };
    (context.finish_analysis(query).unwrap(), context)
}
