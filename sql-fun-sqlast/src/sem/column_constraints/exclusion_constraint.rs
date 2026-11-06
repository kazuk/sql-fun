use crate::{
    names::{ColumnName, TableName},
    sem::{
        ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnRename, ParseContext,
    },
};

// FIXME: exclusion constraint applies column?
/// exclusion column constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct ExclusionColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
}

impl ExclusionColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
        }
    }
}

impl ColumnConstraintTrait for ExclusionColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Exclusion
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnConstraint {
    /// create Exclusion variant
    pub const fn exclusion(value: ExclusionColumnConstraint) -> Self {
        Self::Exclusion(value)
    }

    /// get Exclusion variant
    pub const fn as_exclusion(&self) -> Option<&ExclusionColumnConstraint> {
        if let Self::Exclusion(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

// Analysis owned by the semantic definitions in this module.

impl ExclusionColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_declaration(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Self {
        Self::new(table, column)
    }
}
