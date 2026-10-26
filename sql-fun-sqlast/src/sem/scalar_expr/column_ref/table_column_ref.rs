use crate::sem::{
    ColumnDefinition, ColumnName, Nullable, TableName, TypeReference, data_source::AliasName,
};

/// table column reference
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableColumnRef {
    alias: AliasName,
    column: ColumnName,
    table_name: TableName,
    column_def: Option<ColumnDefinition>,
    datasource_nullable: Nullable,
}

impl super::ColumnReference for TableColumnRef {
    fn get_type(&self) -> Option<TypeReference> {
        if let Some(col_def) = &self.column_def {
            col_def.get_type_cloned()
        } else {
            None
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        if self.datasource_nullable {
            return Some(false);
        }
        let Some(column_def) = &self.column_def else {
            // column definition not resolved, returns Unknown
            return None;
        };
        // returns column nullability
        column_def.is_not_null()
    }

    fn get_column_def(&self) -> &Option<ColumnDefinition> {
        &self.column_def
    }

    fn get_column_name(&self) -> &ColumnName {
        &self.column
    }
}

impl TableColumnRef {
    pub(super) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
        if let Some(column) = &mut self.column_def {
            column.rename_relation(old, new);
        }
    }

    pub(super) fn with_reference_name(mut self, alias: &AliasName, column: &ColumnName) -> Self {
        self.alias = alias.clone();
        self.column = column.clone();
        self
    }

    pub(super) fn canonicalize_grouping_key(&mut self) {
        self.column_def = None;
    }
    /// Exact relation identity for this resolved column.
    pub const fn relation(&self) -> &crate::sem::FullName {
        self.table_name.full_name()
    }
    /// Local source alias, including trigger OLD/NEW aliases.
    pub const fn alias(&self) -> &AliasName {
        &self.alias
    }
    pub(super) fn belongs_to(&self, relation: &crate::sem::FullName) -> bool {
        self.table_name.full_name() == relation
    }

    pub(in crate::sem) fn canonicalize_alias(&mut self) {
        self.alias = AliasName::from(self.table_name.full_name().local_name().as_str());
    }

    /// create instance
    #[must_use]
    pub fn new(
        alias: &AliasName,
        column: &ColumnName,
        table_name: &TableName,
        column_def: Option<&ColumnDefinition>,
        datasource_nullable: Nullable,
    ) -> Self {
        Self {
            alias: alias.clone(),
            column: column.clone(),
            table_name: table_name.clone(),
            column_def: column_def.cloned(),
            datasource_nullable,
        }
    }
}

impl TableColumnRef {
    pub(super) const fn projection_reference(&self) -> Option<(&str, &crate::sem::ColumnName)> {
        Some((self.alias.as_str(), &self.column))
    }
}
