//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{ConflictClause as AstConflictClause, Validity as AstValidity},
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::ColumnNullabilityConstraint,
};

impl ColumnNullabilityConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_parts(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        not_null: bool,
        conflict: &Option<AstConflictClause>,
        validity: &Option<AstValidity>,
    ) -> Self {
        Self::new(table, column, not_null, conflict.clone(), validity.clone())
    }
}
