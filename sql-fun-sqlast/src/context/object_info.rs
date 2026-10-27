use crate::sem::{FullName, Kind};

/// Read the kind at an exact qualified identity, without search-path expansion.
/// Mutation belongs exclusively to definition write/update operations.
pub trait ObjectInfoRead {
    /// Effective table and view catalog identities available for dependency updates.
    fn relation_names(&self) -> Vec<FullName> {
        Vec::new()
    }

    /// Relation kind, including inherited definitions.
    fn object_kind(&self, name: &FullName) -> Option<Kind>;
}

/// Read extension existence in its separate database-wide namespace.
pub trait ExtensionInfoRead {
    /// Whether the current or inherited catalog contains this extension name.
    fn extension_exists(&self, name: &str) -> bool;
}
