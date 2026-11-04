#[cfg(feature = "sqlite")]
use crate::ast::Expr;
use crate::ast::{ColumnConstraint, TableConstraint};

impl TableConstraint {
    pub(crate) fn unnamed_check(constraint: &ColumnConstraint) -> Self {
        Self {
            extras: Default::default(),
            name: None,
            keys: Vec::new(),
            constraint: constraint.clone(),
            order: Vec::new(),
            auto_increment: false,
        }
    }

    pub(crate) const fn is_check(&self) -> bool {
        matches!(self.constraint, ColumnConstraint::Check(_))
    }
}

impl ColumnConstraint {
    #[cfg(feature = "sqlite")]
    pub(crate) fn check(expression: &Expr) -> Self {
        Self::Check(expression.clone())
    }
}
