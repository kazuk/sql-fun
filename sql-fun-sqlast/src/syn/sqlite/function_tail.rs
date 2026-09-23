use turso_parser::ast::FunctionTail as FunctionTailIn;

use crate::syn::sqlite::{Expr, Over};

/// function tail
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct FunctionTail {
    filter_clause: Option<Box<Expr>>,
    over_clause: Option<Over>,
}

impl From<FunctionTailIn> for FunctionTail {
    fn from(value: FunctionTailIn) -> Self {
        Self {
            filter_clause: value.filter_clause.map(|expr| Box::new(Expr::from(*expr))),
            over_clause: value.over_clause.map(Over::from),
        }
    }
}
