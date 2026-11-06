use crate::sem::ColumnRename;
use sql_fun_core::IVec;

use crate::{
    names::{ColumnName, TableName},
    sem::{AnalysisError, CreateIndex, ImplicitChange, create_index::IndexElementCollection},
};

/// analyzed primary key constraint
#[derive(Debug, Clone)]
pub struct PrimaryKeyConstraint {
    name: String,
    table: Option<TableName>,
    keys: IVec<ColumnName>,
}

impl PrimaryKeyConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            let mut keys: Vec<_> = self.keys.iter().cloned().collect();
            rename.rename_names(&mut keys);
            self.keys = keys.into();
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        if let Some(table) = &mut self.table {
            table.rename_reference(old, new);
        }
    }

    pub(crate) fn new(name: &str, table: &TableName, keys: &[ColumnName]) -> Self {
        Self {
            name: name.to_owned(),
            table: Some(table.clone()),
            keys: keys.to_vec().into(),
        }
    }
    /// primary key name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// returns index definition for this primary key
    pub fn implicit_changes(&self) -> Result<Vec<ImplicitChange>, AnalysisError> {
        let Some(table) = &self.table else {
            AnalysisError::raise_unexpected_none("constraint.table")?
        };
        let key_columns = IndexElementCollection::from_column_names(&self.keys);

        Ok(vec![ImplicitChange::from(CreateIndex::new(
            &crate::names::FullName::new(
                &table.full_name().catalog().cloned(),
                &table.full_name().schema().cloned(),
                &crate::names::LocalName::from(self.name.as_str()),
            ),
            table,
            &key_columns,
        ))])
    }

    /// list of primary key columns
    #[must_use]
    pub const fn keys(&self) -> &IVec<ColumnName> {
        &self.keys
    }
}
