use crate::{
    names::{ColumnName, TableName},
    sem::{
        ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnRename, ParseContext,
    },
};

/// deferrable / not deferrable constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct AttrDeferrabilityColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    deferrable: bool,
}

impl AttrDeferrabilityColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName, deferrable: bool) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            deferrable,
        }
    }
}

impl ColumnConstraintTrait for AttrDeferrabilityColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::AttrDeferrability
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnConstraint {
    /// create AttrDeferrability variant
    pub const fn attr_deferrability(value: AttrDeferrabilityColumnConstraint) -> Self {
        Self::AttrDeferrability(value)
    }

    /// get AttrDeferrability variant
    pub const fn as_attr_deferrability(&self) -> Option<&AttrDeferrabilityColumnConstraint> {
        if let Self::AttrDeferrability(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

/// deferred constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct AttrDeferredColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
}

impl AttrDeferredColumnConstraint {
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

impl ColumnConstraintTrait for AttrDeferredColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::AttrDeferred
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnConstraint {
    /// create AttrDeferred variant
    pub const fn attr_deferred(value: AttrDeferredColumnConstraint) -> Self {
        Self::AttrDeferred(value)
    }

    /// get AttrDeferred variant
    pub const fn as_attr_deferred(&self) -> Option<&AttrDeferredColumnConstraint> {
        if let Self::AttrDeferred(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

/// immediate constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct AttrImmediateColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
}

impl AttrImmediateColumnConstraint {
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

impl ColumnConstraintTrait for AttrImmediateColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::AttrImmediate
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnConstraint {
    /// create AttrImmediate variant
    pub const fn attr_immediate(value: AttrImmediateColumnConstraint) -> Self {
        Self::AttrImmediate(value)
    }

    /// get AttrImmediate variant
    pub const fn as_attr_immediate(&self) -> Option<&AttrImmediateColumnConstraint> {
        if let Self::AttrImmediate(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

// Analysis owned by the semantic definitions in this module.

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
