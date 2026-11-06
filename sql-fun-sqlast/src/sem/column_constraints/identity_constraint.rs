use crate::{
    ast::IdentityColumnMode,
    names::{ColumnName, TableName},
    sem::{
        ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnRename, ParseContext,
    },
};

/// identity column constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct IdentityColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    generated_when: IdentityColumnMode,
}

impl IdentityColumnConstraint {
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
        generated_when: IdentityColumnMode,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            generated_when,
        }
    }
}

impl ColumnConstraintTrait for IdentityColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Identity
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl IdentityColumnConstraint {
    /// Whether explicit values require an OVERRIDING clause.
    pub const fn is_always(&self) -> bool {
        matches!(self.generated_when, IdentityColumnMode::Always)
    }
}

impl ColumnConstraint {
    /// create identity variant
    pub const fn identity(value: IdentityColumnConstraint) -> Self {
        Self::Identity(value)
    }

    /// get identity variant
    pub const fn as_identity(&self) -> Option<&IdentityColumnConstraint> {
        if let Self::Identity(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

// Analysis owned by the semantic definitions in this module.

impl IdentityColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_mode(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        mode: &IdentityColumnMode,
    ) -> Self {
        Self::new(table, column, mode.clone())
    }
}
