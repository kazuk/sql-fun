//! Parser and catalog fixtures for operator identity contracts.

use crate::{
    ast::Name,
    context::ParseContext,
    names::{OperatorName, SchemaName},
    sem::{
        AnalysisError, BinaryOperatorDefinition, OperatorDefinition, PgBuiltInType,
        SelectStatement, UnaryOperatorDefinition,
    },
    syn::{ListOpt, Node, NodeListOpt},
    test_helpers::{
        SynBuilder, TestParseContext,
        ast_query::{AnalysisFixture, postgres},
        create_cache_context,
    },
};

pub(crate) fn parsed_name(sql: &str) -> Name {
    let nodes = SynBuilder::new()
        .parse(sql)
        .as_select_stmt()
        .get_target_list()
        .get(0)
        .as_res_target()
        .get_val()
        .as_a_expr()
        .get_name();
    let parts = nodes
        .map_values(|node| node.as_string().get_sval())
        .expect("operator names");
    Name::from_parts(&parts)
}

/// Keep fabricated malformed parser-node cases out of semantic tests.
pub(crate) fn malformed_names() -> Vec<Name> {
    let builder = SynBuilder::new();
    [
        builder.empty_node_list(),
        builder.as_node_list(&[Node::none()]),
        builder.string_node_list(&[""]),
        builder.as_node_list(&[
            builder.as_string_node(builder.string("a")),
            Node::none(),
            builder.as_string_node(builder.string("+")),
        ]),
    ]
    .into_iter()
    .map(|nodes| {
        let nodes = NodeListOpt::from(&nodes);
        let components = nodes.as_inner().unwrap_or_default();
        let malformed_node = components
            .iter()
            .any(|node| node.as_string().get_sval().is_none());
        let converted = Name::try_from(nodes);
        if malformed_node {
            assert!(
                converted.is_err(),
                "non-string parser node must be rejected"
            );
        } else {
            assert!(
                converted.is_ok(),
                "string components must reach name validation"
            );
        }
        let parts = components
            .into_iter()
            .map(|node| node.as_string().get_sval().unwrap_or_default())
            .collect::<Vec<_>>();
        Name::from_parts(&parts)
    })
    .collect()
}

pub(crate) fn addition_context(schema: Option<&str>) -> TestParseContext {
    let mut context = AnalysisFixture::context();
    let integer = PgBuiltInType::int4();
    let definition = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
        &integer, &integer, &integer, true,
    )]);
    context.set_get_operator_op_result(
        &OperatorName::from_parts(schema, "+"),
        true,
        true,
        Some(&definition),
    );
    context
}

pub(crate) fn analyze(
    sql: &str,
    mut context: TestParseContext,
) -> Result<(SelectStatement, TestParseContext), AnalysisError> {
    context.begin_parameter_type_analysis();
    let query = SelectStatement::from_ast_test_in(&postgres(sql)?, &mut context, None);
    let query = context.infer_query_parameter_types(query, &mut create_cache_context());
    let query = context.finish_analysis(query)?;
    Ok((query, context))
}

pub(crate) fn lookup_context(path: &[&str], left: bool, right: bool) -> TestParseContext {
    let mut context = addition_context(None);
    let path: Vec<_> = path.iter().map(|schema| SchemaName::from(schema)).collect();
    context.set_search_path(&path);
    let integer = PgBuiltInType::int4();
    let unary = UnaryOperatorDefinition::new(&integer, &integer, true);
    let definition = if left && right {
        OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
            &integer, &integer, &integer, true,
        )])
    } else if left {
        OperatorDefinition::left_unary(&[unary])
    } else {
        OperatorDefinition::right_unary(&[unary])
    };
    for schema in ["a", "b", "a.b", "Mixed", "pg_catalog"] {
        context.set_get_operator_op_result(
            &OperatorName::from_parts(Some(schema), "+"),
            left,
            right,
            Some(&definition),
        );
    }
    context
}
