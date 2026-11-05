//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{ConflictClause as AstConflictClause, Deferrable as AstDeferrable},
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::PrimaryKeyColumnConstraint,
};

impl PrimaryKeyColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_parts(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        conflict: &Option<AstConflictClause>,
        deferrable: &Option<AstDeferrable>,
    ) -> Self {
        Self::new(table, column, conflict.clone(), deferrable.clone())
    }
}
