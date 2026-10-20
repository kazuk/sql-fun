use crate::sem::{
    ColumnDefinition, ColumnName, CteName, Nullable, TypeReference, data_source::AliasName,
};

/// Column reference for CTE source
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CteColumnRef {
    alias: AliasName,
    cte_name: CteName,
    column_name: ColumnName,
    column_def: Option<ColumnDefinition>,
    source_nullable: Nullable,
}

impl super::ColumnReference for CteColumnRef {
    fn get_type(&self) -> Option<TypeReference> {
        if let Some(col_def) = &self.column_def {
            col_def.get_type_cloned()
        } else {
            None
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        if self.source_nullable {
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
        &self.column_name
    }
}

impl CteColumnRef {
    pub(super) fn with_reference_name(mut self, alias: &AliasName, column: &ColumnName) -> Self {
        self.alias = alias.clone();
        self.column_name = column.clone();
        self
    }

    pub(super) fn canonicalize_grouping_key(&mut self) {
        self.column_def = None;
    }
    pub(super) const fn binding_alias(&self) -> &AliasName {
        &self.alias
    }

    /// create instance
    #[must_use]
    pub fn new(
        alias: &AliasName,
        cte_name: &CteName,
        column_name: &ColumnName,
        column_def: Option<&ColumnDefinition>,
        source_nullable: Nullable,
    ) -> Self {
        Self {
            alias: alias.clone(),
            cte_name: cte_name.clone(),
            column_name: column_name.clone(),
            column_def: column_def.cloned(),
            source_nullable,
        }
    }
}
