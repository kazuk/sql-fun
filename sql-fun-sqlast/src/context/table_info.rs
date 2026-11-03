use crate::{
    names::FullName,
    sem::{AlterTable, AnalysisError, CreateTable, DropTable},
};

/// get table definition
pub trait TableInfoRead {
    /// get `CREATE TABLE` statement by `full_name`
    fn get_table_impl(&self, key: &FullName) -> Option<&CreateTable>;
}

/// table information writer
pub trait TableInfoWrite
where
    Self: Sized,
{
    /// apply `alter table` statement to current schema
    fn apply_alter_table(self, alter_table: &AlterTable) -> Result<Self, AnalysisError>;

    /// apply `drop table` statement to current schema
    ///
    /// # Note
    ///
    /// `sql-fun` only records `DROP TABLE` statement existing.
    ///
    /// Schema dump is a Source of Truth, can not to change table existing state.
    ///
    fn apply_drop_table(self, drop_table: &DropTable) -> Result<Self, AnalysisError>;

    /// insert create table statement
    fn insert_create_table(&mut self, name: &FullName, create_table: &CreateTable);
}
