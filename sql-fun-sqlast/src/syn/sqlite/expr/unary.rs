#[cfg(feature = "sqlite")]
use crate::syn::sqlite::expr::Expr;

/// unary expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprUnary {
    unary_operator: UnaryOperator,
    expr: Box<Expr>,
}

impl ExprUnary {
    /// create instance
    pub fn new(unary_operator: UnaryOperator, expr: Expr) -> Self {
        Self {
            unary_operator,
            expr: Box::new(expr),
        }
    }
}

impl Expr {
    /// create Unary variant
    pub fn unary<Ope, Value>(unary_operator: Ope, expr: Value) -> Self
    where
        Ope: Into<UnaryOperator>,
        Value: Into<Self>,
    {
        Self::Unary(ExprUnary::new(unary_operator.into(), expr.into()))
    }
}

use turso_parser::ast::UnaryOperator as UnaryOperatorIn;

/// unary operator
#[derive(Clone, Debug)]
pub enum UnaryOperator {
    /// bitwise not operator
    BitwiseNot,
    /// negative value
    Negative,
    /// logical not operator
    Not,
    /// positive value
    Positive,
}

impl From<UnaryOperatorIn> for UnaryOperator {
    fn from(value: UnaryOperatorIn) -> Self {
        match value {
            UnaryOperatorIn::BitwiseNot => Self::BitwiseNot,
            UnaryOperatorIn::Negative => Self::Negative,
            UnaryOperatorIn::Not => Self::Not,
            UnaryOperatorIn::Positive => Self::Positive,
        }
    }
}
