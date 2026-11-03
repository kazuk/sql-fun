use crate::names::SchemaName;

/// search path
pub trait SearchPathRead {
    /// get search path
    fn get_search_path(&self) -> &[SchemaName];

    /// gets default schema.
    ///
    /// # Note
    ///
    /// returns first item of current search path
    fn get_default_schema(&self) -> Option<SchemaName>;
}
