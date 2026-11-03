#[cfg(feature = "sqlite")]
use crate::syn::sqlite::ResolveType;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::expr::Expr;

/// raise expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprRaise {
    resolve_type: ResolveType,
    expr: Option<Box<Expr>>,
}

impl ExprRaise {
    /// create instance
    pub fn new(resolve_type: ResolveType, expr: Option<Expr>) -> Self {
        Self {
            resolve_type,
            expr: expr.map(Box::new),
        }
    }
}

impl Expr {
    /// create Raise variant
    pub fn raise<Res, Ex>(resolve_type: Res, expr: Option<Ex>) -> Self
    where
        Res: Into<ResolveType>,
        Ex: Into<Self>,
    {
        Self::Raise(ExprRaise::new(resolve_type.into(), expr.map(|v| v.into())))
    }
}
