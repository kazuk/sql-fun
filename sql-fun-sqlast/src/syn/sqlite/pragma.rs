use crate::syn::sqlite::{Expr, QualifiedName};

/// pragma statement
#[derive(derive_getters::Getters, Debug)]
pub struct PragmaStatement {
    name: QualifiedName,
    body: Option<PragmaBody>,
}

impl PragmaStatement {
    /// create instance
    pub const fn new(name: QualifiedName, body: Option<PragmaBody>) -> Self {
        Self { name, body }
    }
}

use turso_parser::ast::PragmaBody as PragmaBodyIn;

/// pragma statement body
#[derive(Debug)]
pub enum PragmaBody {
    /// equals
    Equals(Box<Expr>),
    /// call
    Call(Box<Expr>),
}

impl From<PragmaBodyIn> for PragmaBody {
    fn from(value: PragmaBodyIn) -> Self {
        match value {
            PragmaBodyIn::Equals(expr) => Self::Equals(Box::new(Expr::from(*expr))),
            PragmaBodyIn::Call(expr) => Self::Call(Box::new(Expr::from(*expr))),
        }
    }
}
