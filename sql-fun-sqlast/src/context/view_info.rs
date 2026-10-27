use crate::sem::{AlterView, AnalysisError, CreateTableAs, CreateView, FullName, ViewLike};
use crate::sem::{ViewDeleteSupport, ViewInsertSupport, ViewMergeSupport, ViewUpdateSupport};

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

/// Catalog service for view destinations. The name is already bound and
/// qualified; this is not a second search-path lookup.
pub trait ViewInsertInfoRead {
    /// Retrieve explicitly registered support by exact identity, without deriving
    /// capabilities or interpreting absence. The analyzer owns those decisions.
    fn view_insert_support(&self, view: &FullName) -> Option<&ViewInsertSupport>;
}

/// Catalog service for view destinations. The name is already bound and
/// qualified; this is not a second search-path lookup.
pub trait ViewUpdateInfoRead {
    /// Retrieve explicitly registered support by exact identity, without deriving
    /// capabilities or interpreting absence. The analyzer owns those decisions.
    fn view_update_support(&self, view: &FullName) -> Option<&ViewUpdateSupport>;
}

/// DELETE capability service for an already bound, qualified ordinary view.
pub trait ViewDeleteInfoRead {
    /// Return a completed decision without executing trigger or rule bodies.
    /// Catalogs without the required metadata must leave this explicitly
    /// unimplemented, rather than inventing success or a read-only decision.
    fn view_delete_support(&self, view: &FullName) -> ViewDeleteSupport;
}

/// Catalog service for MERGE view targets. The name is already bound and
/// qualified; this is not a second search-path lookup.
pub trait ViewMergeInfoRead {
    /// Return explicit MERGE eligibility and action-column metadata.
    ///
    /// Implementations must account for view updatability, rules and
    /// action-specific INSTEAD OF triggers. Missing metadata must remain
    /// explicitly unimplemented rather than being interpreted as success.
    fn view_merge_support(&self, view: &FullName) -> ViewMergeSupport;
}
