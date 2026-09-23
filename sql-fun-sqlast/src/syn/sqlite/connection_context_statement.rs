use crate::syn::sqlite::Expr;

/// attach statement
#[derive(derive_getters::Getters, Debug)]
pub struct AttachStatement {
    expr: Box<Expr>,
    db_name: Box<Expr>,
    key: Option<Box<Expr>>,
}

impl AttachStatement {
    /// create instance
    pub fn new(expr: Expr, db_name: Expr, key: Option<Expr>) -> Self {
        Self {
            expr: Box::new(expr),
            db_name: Box::new(db_name),
            key: key.map(Box::new),
        }
    }
}

/// detach statement
#[derive(derive_getters::Getters, Debug)]
pub struct DetachStatement {
    db_name: Box<Expr>,
}

impl DetachStatement {
    /// create instance
    pub fn new(db_name: Expr) -> Self {
        Self {
            db_name: Box::new(db_name),
        }
    }
}
