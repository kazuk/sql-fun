use crate::context::{FrozenBaseContext, ParseContext as _};
use crate::test_helpers::schema_layer;

use crate::context::SchemaFileContext;
use crate::sem::{
    BinaryOperatorDefinition, OperatorDefinition, PgBuiltInType, TypeReference,
    UnaryOperatorDefinition,
};
use crate::test_helpers::TestParseContext;
use std::path::Path;

pub(crate) fn operator_layer(has_left: bool, has_right: bool) -> FrozenBaseContext {
    let root = TestParseContext::default().freeze();
    let mut parent = schema_layer(Path::new("parent.sql"), &root);
    register(
        &mut parent,
        "+",
        has_left,
        has_right,
        &PgBuiltInType::int4(),
    );
    register(
        &mut parent,
        "-",
        has_left,
        has_right,
        &PgBuiltInType::int4(),
    );
    let mut child = schema_layer(Path::new("child.sql"), &parent.freeze());
    register(&mut child, "+", has_left, has_right, &PgBuiltInType::int8());
    child.freeze()
}

fn register(
    context: &mut SchemaFileContext,
    name: &str,
    left: bool,
    right: bool,
    result: &TypeReference,
) {
    let argument = PgBuiltInType::int4();
    if left && right {
        let overload = BinaryOperatorDefinition::new(&argument, &argument, result, true);
        context.set_test_operator(name, OperatorDefinition::binary(&[overload]));
    } else if left {
        let overload = UnaryOperatorDefinition::new(&argument, result, true);
        context.set_test_operator(name, OperatorDefinition::left_unary(&[overload]));
    } else {
        let overload = UnaryOperatorDefinition::new(&argument, result, true);
        context.set_test_operator(name, OperatorDefinition::right_unary(&[overload]));
    }
}

pub(crate) fn assert_returns(operator: &OperatorDefinition, expected: &TypeReference) {
    match operator {
        OperatorDefinition::Binary(overloads) => {
            assert_eq!(overloads.len(), 1);
            assert_eq!(overloads[0].get_return_type(), expected);
        }
        OperatorDefinition::LeftUnary(overloads) | OperatorDefinition::RightUnary(overloads) => {
            assert_eq!(overloads.len(), 1);
            assert_eq!(overloads[0].get_return_type(), expected);
        }
    }
}
