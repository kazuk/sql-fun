use crate::sem::{AlterView, AnalysisError, CreateTableAs, CreateView, FullName, ViewLike};

/// get view definition
pub trait ViewInfoRead {
    /// Resolve one exact SQLite trigger identity to its owning view, without
    /// exposing trigger storage or searching another schema.
    fn view_trigger_target(&self, trigger: &FullName) -> Option<FullName>;

    /// Read write-analysis facts for one exact view identity.
    /// This service does not search the search path or expose catalog storage.
    fn view_write_info(&self, view: &FullName) -> Option<&crate::sem::ViewWriteInfo>;
    /// get `CREATE VIEW` statement by view name
    fn get_view_impl(&self, key: &FullName) -> Option<&ViewLike>;
}

/// store update view definitions
pub trait ViewInfoWrite
where
    Self: Sized,
{
    /// Retain a completed view definition for write analysis.
    fn apply_view_write_info(
        self,
        view: &FullName,
        info: &crate::sem::ViewWriteInfo,
    ) -> Result<Self, AnalysisError>;

    /// Apply one bound trigger, rule or default change.
    fn apply_view_write_change(
        self,
        change: &crate::sem::ViewWriteChange,
    ) -> Result<Self, AnalysisError>;

    /// apply `alter view` statement to current schema
    fn apply_alter_view(self, alter_table: &AlterView) -> Result<Self, AnalysisError>;

    /// apply `create view` to current schema
    fn apply_create_view(self, create_view: &CreateView) -> Result<Self, AnalysisError>;

    /// apply `create table as` statement to current schema
    fn apply_create_table_as(self, create_table_as: &CreateTableAs) -> Result<Self, AnalysisError>;
}
