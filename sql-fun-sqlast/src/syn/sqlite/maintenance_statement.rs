use crate::syn::sqlite::{Expr, Name};

/// vacuum statement
#[derive(derive_getters::Getters)]
pub struct VacuumStatement {
    db_name: Option<Name>,
    into: Option<Box<Expr>>,
}

impl VacuumStatement {
    /// create instance
    pub fn new(db_name: Option<Name>, into: Option<Expr>) -> Self {
        Self {
            db_name,
            into: into.map(Box::new),
        }
    }
}
