use crate::syn::sqlite::{Expr, Indexed, Limit, QualifiedName, ResultColumn, SortedColumn, With};

/// delete statement
#[derive(derive_getters::Getters)]
pub struct DeleteStatement {
    with: Option<With>,
    table_name: QualifiedName,
    indexed: Option<Indexed>,
    where_clause: Option<Box<Expr>>,
    returning: Vec<ResultColumn>,
    order_by: Vec<SortedColumn>,
    limit: Option<Limit>,
}

impl DeleteStatement {
    /// create instance
    pub fn new(
        with: Option<With>,
        table_name: QualifiedName,
        indexed: Option<Indexed>,
        where_clause: Option<Box<Expr>>,
        returning: Vec<ResultColumn>,
        order_by: Vec<SortedColumn>,
        limit: Option<Limit>,
    ) -> Self {
        Self {
            with,
            table_name,
            indexed,
            where_clause,
            returning,
            order_by,
            limit,
        }
    }
}
