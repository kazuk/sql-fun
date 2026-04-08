use crate::syn::sqlite::Expr;

use turso_parser::ast::SortedColumn as SortedColumnIn;

/// sorted column
#[derive(Debug, derive_getters::Getters)]
pub struct SortedColumn {
    expr: Box<Expr>,
    order: Option<SortOrder>,
    nulls: Option<NullsOrder>,
}

impl From<SortedColumnIn> for SortedColumn {
    fn from(value: SortedColumnIn) -> Self {
        Self {
            expr: Box::new(Expr::from(*value.expr)),
            order: value.order.map(SortOrder::from),
            nulls: value.nulls.map(NullsOrder::from),
        }
    }
}

use turso_parser::ast::SortOrder as SortOrderIn;

/// sort order value
#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    /// ascending
    Asc,
    /// descending
    Desc,
}

impl From<SortOrderIn> for SortOrder {
    fn from(value: SortOrderIn) -> Self {
        match value {
            SortOrderIn::Asc => Self::Asc,
            SortOrderIn::Desc => Self::Desc,
        }
    }
}

use turso_parser::ast::NullsOrder as NullsOrderIn;

/// nulls order option
#[derive(Debug)]
pub enum NullsOrder {
    /// first
    First,
    /// last
    Last,
}

impl From<NullsOrderIn> for NullsOrder {
    fn from(value: NullsOrderIn) -> Self {
        match value {
            NullsOrderIn::First => Self::First,
            NullsOrderIn::Last => Self::Last,
        }
    }
}
