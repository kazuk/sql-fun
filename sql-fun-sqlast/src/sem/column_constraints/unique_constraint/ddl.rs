//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::ConflictClause as AstConflictClause,
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::UniqueColumnConstraint,
};

impl UniqueColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_conflict(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        conflict: &Option<AstConflictClause>,
    ) -> Self {
        Self::new(table, column, conflict.clone())
    }
}
