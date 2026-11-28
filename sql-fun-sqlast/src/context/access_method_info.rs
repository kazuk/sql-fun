use crate::sem::AccessMethodDefinition;

/// Reads from the effective database-wide access-method catalog.
pub trait AccessMethodInfoRead {
    /// Authoritative exclusion capability, when supplied by the catalog source.
    fn access_method_can_exclude(&self, _name: &str) -> Option<bool> {
        None
    }

    /// Look up an access method by its normalized SQL identifier.
    fn access_method(&self, _name: &str) -> Option<&AccessMethodDefinition> {
        None
    }
}

/// Writes to a mutable schema layer.
pub trait AccessMethodInfoWrite: Sized {
    /// Publish a completed definition whose name was checked during analysis.
    fn register_access_method(self, definition: &AccessMethodDefinition) -> Self;
}
