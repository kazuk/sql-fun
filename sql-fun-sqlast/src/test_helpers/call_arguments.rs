use crate::test_helpers::query::expression;

use crate::sem::FuncCallExpr;

use crate::sem::SelectStatement;

pub(crate) fn call(query: &SelectStatement) -> &FuncCallExpr {
    expression(query)
        .as_function_call()
        .expect("preserved call")
}
