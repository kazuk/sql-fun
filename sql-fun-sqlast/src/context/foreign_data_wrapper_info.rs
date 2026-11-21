use crate::sem::CreateForeignDataWrapper;

/// Read analyzed foreign-data wrappers by their database-wide identifier.
pub trait ForeignDataWrapperInfoRead {
    /// Look up a visible wrapper. Builtin-only catalogs contain no wrappers.
    fn foreign_data_wrapper(&self, _name: &str) -> Option<&CreateForeignDataWrapper> {
        None
    }
}

/// Register analyzed wrappers in the current catalog layer.
pub trait ForeignDataWrapperInfoWrite: Sized {
    /// Store a validated wrapper definition in this layer.
    fn register_foreign_data_wrapper(self, definition: &CreateForeignDataWrapper) -> Self;
}
