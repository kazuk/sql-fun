use crate::{
    ast::{ConflictClause, Deferrable},
    names::{ColumnName, TableName},
    sem::{
        ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnRename, ParseContext,
    },
};

/// primary key constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct PrimaryKeyColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    conflict_clause: Option<ConflictClause>,
    deferrable: Option<Deferrable>,
}

impl PrimaryKeyColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    /// create instance
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        conflict_clause: Option<ConflictClause>,
        deferrable: Option<Deferrable>,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            conflict_clause,
            deferrable,
        }
    }
}

impl ColumnConstraintTrait for PrimaryKeyColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::PrimaryKey
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnConstraint {
    /// create PrimaryKey variant
    pub const fn primary_key(value: PrimaryKeyColumnConstraint) -> Self {
        Self::PrimaryKey(value)
    }

    /// get PrimaryKey variant
    pub const fn as_primary_key(&self) -> Option<&PrimaryKeyColumnConstraint> {
        if let Self::PrimaryKey(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

// Analysis owned by the semantic definitions in this module.

impl PrimaryKeyColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_parts(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        conflict: &Option<ConflictClause>,
        deferrable: &Option<Deferrable>,
    ) -> Self {
        Self::new(table, column, conflict.clone(), deferrable.clone())
    }
}
