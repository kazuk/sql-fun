//! Analysis owned by the semantic definitions in this module.

use crate::{
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{
        AttrDeferrabilityColumnConstraint, AttrDeferredColumnConstraint,
        AttrImmediateColumnConstraint,
    },
};

impl AttrDeferrabilityColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_value(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        value: bool,
    ) -> Self {
        Self::new(table, column, value)
    }
}

impl AttrDeferredColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_declaration(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Self {
        Self::new(table, column)
    }
}

impl AttrImmediateColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_declaration(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Self {
        Self::new(table, column)
    }
}
