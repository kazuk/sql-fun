use super::Expr;
use std::num::NonZero;
use turso_parser::ast::Variable as VariableIn;

/// variable expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct Variable {
    index: NonZero<u32>,
    name: Option<String>,
    col_type: Option<String>,
}

impl From<VariableIn> for Variable {
    fn from(value: VariableIn) -> Self {
        Self {
            index: value.index,
            name: value.name.map(String::from),
            col_type: value.col_type.map(String::from),
        }
    }
}

impl Expr {
    /// create Variable variant
    pub fn variable<V>(var: V) -> Self
    where
        V: Into<Variable>,
    {
        Self::Variable(var.into())
    }
}
