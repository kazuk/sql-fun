use crate::{
    names::{AliasName, ColumnName},
    sem::{ColumnDefinition, Nullable, TypeReference, source_flag::SourceFlag},
};

/// column reference for sub-query
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SubQueryColumnRef {
    alias: AliasName,
    #[serde(skip)]
    unqualified: SourceFlag,
    column: ColumnName,
    column_def: Option<ColumnDefinition>,
    datasource_nullable: Nullable,
}

impl super::ColumnReference for SubQueryColumnRef {
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
            return None;
        };
        column_def.is_not_null()
    }

    fn get_column_def(&self) -> &Option<ColumnDefinition> {
        &self.column_def
    }

    fn get_column_name(&self) -> &ColumnName {
        &self.column
    }
}

impl SubQueryColumnRef {
    pub(super) fn set_projection_name(&mut self, name: &ColumnName) {
        self.column = name.clone();
        if let Some(column) = &mut self.column_def {
            column.set_projection_name(name);
        }
    }

    pub(super) const fn mark_unqualified(&mut self) {
        self.unqualified = SourceFlag::new(true);
    }
    pub(super) const fn namespace_reference(&self) -> (&str, &ColumnName, bool) {
        (self.alias.as_str(), &self.column, self.unqualified.get())
    }

    pub(super) fn with_reference_name(mut self, alias: &AliasName, column: &ColumnName) -> Self {
        self.alias = alias.clone();
        self.column = column.clone();
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
        column: &ColumnName,
        column_def: Option<&ColumnDefinition>,
        datasource_nullable: Nullable,
    ) -> Self {
        Self {
            alias: alias.clone(),
            unqualified: SourceFlag::new(false),
            column: column.clone(),
            column_def: column_def.cloned(),
            datasource_nullable,
        }
    }
}

impl SubQueryColumnRef {
    pub(super) const fn projection_reference(&self) -> Option<(&str, &ColumnName)> {
        Some((self.alias.as_str(), &self.column))
    }
}
