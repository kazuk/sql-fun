use crate::sem::{
    ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnName, TableName,
};

/// collate constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CollateColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    collation_name: String,
}

impl CollateColumnConstraint {
    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName, collation_name: &str) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            collation_name: String::from(collation_name),
        }
    }

    /// get collation name
    pub fn collation_name(&self) -> &str {
        &self.collation_name
    }
}

impl ColumnConstraintTrait for CollateColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Collate
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnConstraint {
    /// create Collate variant
    pub fn collate(value: CollateColumnConstraint) -> Self {
        Self::Collate(value)
    }

    /// get Collate variant
    pub fn as_collate(&self) -> Option<&CollateColumnConstraint> {
        if let Self::Collate(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
