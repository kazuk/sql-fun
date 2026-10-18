use crate::sem::{AlterView, AnalysisError, CreateTableAs, CreateView, FullName, ViewLike};

/// get view definition
pub trait ViewInfoRead {
    /// get `CREATE VIEW` statement by view name
    fn get_view_impl(&self, key: &FullName) -> Option<&ViewLike>;
}

/// store update view definitions
pub trait ViewInfoWrite
where
    Self: Sized,
{
    /// apply `alter view` statement to current schema
    fn apply_alter_view(self, alter_table: &AlterView) -> Result<Self, AnalysisError>;

    /// apply `create view` to current schema
    fn apply_create_view(self, create_view: &CreateView) -> Result<Self, AnalysisError>;

    /// apply `create table as` statement to current schema
    fn apply_create_table_as(self, create_table_as: &CreateTableAs) -> Result<Self, AnalysisError>;
}
