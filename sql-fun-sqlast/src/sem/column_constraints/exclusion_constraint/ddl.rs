//! Analysis owned by the semantic definitions in this module.

use crate::{
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::ExclusionColumnConstraint,
};

impl ExclusionColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_declaration(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Self {
        Self::new(table, column)
    }
}
