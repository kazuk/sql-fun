use crate::sem::{ColumnDefinition, ColumnName, TypeReference};

/// A name or ordinal bound to a projection position in the current query.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OutputColumnRef {
    index: usize,
    definition: Option<ColumnDefinition>,
    name: ColumnName,
}
impl OutputColumnRef {
    pub(super) fn new(index: usize, definition: &ColumnDefinition) -> Self {
        Self {
            index,
            definition: Some(definition.clone()),
            name: definition
                .name()
                .clone()
                .unwrap_or_else(|| ColumnName::from("")),
        }
    }
    /// Zero-based position in the query's output.
    pub const fn index(&self) -> usize {
        self.index
    }
    pub(super) fn get_type(&self) -> Option<TypeReference> {
        self.definition.as_ref()?.get_type_cloned()
    }
    pub(super) fn is_not_null(&self) -> Option<bool> {
        self.definition.as_ref()?.is_not_null()
    }
    pub(super) const fn get_column_def(&self) -> &Option<ColumnDefinition> {
        &self.definition
    }
    pub(super) const fn get_column_name(&self) -> &ColumnName {
        &self.name
    }
}
