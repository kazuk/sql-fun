use super::Expr;
use crate::syn::sqlite::Name;

/// collate expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprCollate {
    expr: Box<Expr>,
    name: Name,
}

impl ExprCollate {
    /// create instance
    pub fn new(expr: Expr, name: Name) -> Self {
        Self {
            expr: Box::new(expr),
            name,
        }
    }
}

impl Expr {
    /// create collate variant
    pub fn collate<EXPR, NAME>(expr: EXPR, name: NAME) -> Self
    where
        EXPR: Into<Self>,
        NAME: Into<Name>,
    {
        Self::Collate(ExprCollate::new(expr.into(), name.into()))
    }
}
