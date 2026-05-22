use super::Expr;
use crate::syn::sqlite::{Distinctness, FunctionTail, Name, SortedColumn};

/// function call expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprFunctionCall {
    name: Name,
    distinctness: Option<Distinctness>,
    args: Vec<Box<Expr>>,
    order_by: Vec<SortedColumn>,
    filter_over: FunctionTail,
}

impl ExprFunctionCall {
    /// create instance
    pub fn new(
        name: Name,
        distinctness: Option<Distinctness>,
        args: Vec<Expr>,
        order_by: Vec<SortedColumn>,
        filter_over: FunctionTail,
    ) -> Self {
        Self {
            name,
            distinctness,
            args: args.into_iter().map(Box::new).collect(),
            order_by,
            filter_over,
        }
    }
}

impl Expr {
    /// create FunctionCall variant
    pub fn function_call<NAME, DISTINCT, ARG, SORT, TAIL>(
        name: NAME,
        distinctness: Option<DISTINCT>,
        args: Vec<Box<ARG>>,
        order_by: Vec<SORT>,
        filter_over: TAIL,
    ) -> Self
    where
        NAME: Into<Name>,
        DISTINCT: Into<Distinctness>,
        ARG: Into<Expr>,
        SORT: Into<SortedColumn>,
        TAIL: Into<FunctionTail>,
    {
        Self::FunctionCall(ExprFunctionCall::new(
            name.into(),
            distinctness.map(|v| v.into()),
            args.into_iter().map(|v| (*v).into()).collect(),
            order_by.into_iter().map(|v| v.into()).collect(),
            filter_over.into(),
        ))
    }
}

/// function call star expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprFunctionCallStar {
    name: Name,
    filter_over: FunctionTail,
}

impl ExprFunctionCallStar {
    /// create instance
    pub fn new(name: Name, filter_over: FunctionTail) -> Self {
        Self { name, filter_over }
    }
}

impl Expr {
    /// create FunctionCallStar variant
    pub fn function_call_star<NAME, TAIL>(name: NAME, filter_over: TAIL) -> Self
    where
        NAME: Into<Name>,
        TAIL: Into<FunctionTail>,
    {
        Self::FunctionCallStar(ExprFunctionCallStar::new(name.into(), filter_over.into()))
    }
}
