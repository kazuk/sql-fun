use crate::sem::{AlterTable, AnalysisError, CreateTable, FullName};

/// get table definition
pub trait TableInfoRead {
    /// get `CREATE TABLE` statement by full_name
    fn get_table_impl(&self, key: &FullName) -> Option<&CreateTable>;
}

/// table information writer
pub trait TableInfoWrite
where
    Self: Sized,
{
    /// apply `alter table` statement to current schema
    fn apply_alter_table(self, alter_table: &AlterTable) -> Result<Self, AnalysisError>;

    /// insert create table statement
    fn insert_create_table(&mut self, name: &FullName, create_table: &CreateTable);
}
