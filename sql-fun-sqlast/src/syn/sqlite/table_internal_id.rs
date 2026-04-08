use turso_parser::ast::TableInternalId as TableInternalIdIn;

/// Table internal id
#[derive(Debug)]
pub struct TableInternalId(TableInternalIdIn);

impl From<TableInternalIdIn> for TableInternalId {
    fn from(value: TableInternalIdIn) -> Self {
        Self(value)
    }
}

impl TableInternalId {
    /// value
    pub fn value(&self) -> usize {
        self.0.into()
    }
}
