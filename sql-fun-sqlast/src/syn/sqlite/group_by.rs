use turso_parser::ast::GroupBy as GroupByIn;

use crate::syn::sqlite::Expr;

/// group by clause
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct GroupBy {
    exprs: Vec<Expr>,
    having: Option<Box<Expr>>,
}

impl From<GroupByIn> for GroupBy {
    fn from(value: GroupByIn) -> Self {
        Self {
            exprs: value.exprs.into_iter().map(|v| Expr::from(*v)).collect(),
            having: value.having.map(|v| Box::new(Expr::from(*v))),
        }
    }
}
