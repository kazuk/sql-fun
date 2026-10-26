use crate::sem::{CreateIndex, FullName, TableName};

/// get index definition
pub trait IndexInfoRead {
    /// Enumerate indexes belonging to the qualified destination table.
    fn indexes_for_table(&self, table: &TableName) -> Vec<&CreateIndex>;

    /// Read an index by its exact qualified identity, without search-path expansion.
    fn get_index_impl(&self, index_name: &FullName) -> Option<&CreateIndex>;
}
