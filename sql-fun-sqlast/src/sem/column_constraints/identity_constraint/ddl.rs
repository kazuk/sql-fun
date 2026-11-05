//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::IdentityColumnMode as AstIdentityColumnMode,
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::IdentityColumnConstraint,
};

impl IdentityColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_mode(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        mode: &AstIdentityColumnMode,
    ) -> Self {
        Self::new(table, column, mode.clone())
    }
}
