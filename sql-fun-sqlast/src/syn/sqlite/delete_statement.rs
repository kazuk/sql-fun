use crate::syn::sqlite::{Expr, Indexed, QualifiedName, ResultColumn, With};

/// delete statement
#[derive(derive_getters::Getters, Debug)]
pub struct DeleteStatement {
    with: Option<With>,
    table_name: QualifiedName,
    indexed: Option<Indexed>,
    where_clause: Option<Box<Expr>>,
    returning: Vec<ResultColumn>,
}

impl DeleteStatement {
    /// create instance
    pub const fn new(
        with: Option<With>,
        table_name: QualifiedName,
        indexed: Option<Indexed>,
        where_clause: Option<Box<Expr>>,
        returning: Vec<ResultColumn>,
    ) -> Self {
        Self {
            with,
            table_name,
            indexed,
            where_clause,
            returning,
        }
    }
}
