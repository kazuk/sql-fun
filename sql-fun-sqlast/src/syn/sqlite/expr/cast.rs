use super::Expr;
use crate::syn::sqlite::Type;

/// cast expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprCast {
    expr: Box<Expr>,
    type_name: Option<Type>,
}

impl ExprCast {
    /// create instance
    pub fn new(expr: Expr, type_name: Option<Type>) -> Self {
        Self {
            expr: Box::new(expr),
            type_name,
        }
    }
}

impl Expr {
    /// create cast variant
    pub fn cast<EXPR, TY>(expr: EXPR, type_name: Option<TY>) -> Self
    where
        EXPR: Into<Self>,
        TY: Into<Type>,
    {
        Self::Cast(ExprCast::new(expr.into(), type_name.map(|v| v.into())))
    }
}
