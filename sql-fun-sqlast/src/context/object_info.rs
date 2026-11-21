use crate::{
    names::FullName,
    sem::{ExtensionMembershipChange, Kind, OwnershipIdentity},
};

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

    /// Schema associated with the installed extension, when known.
    fn extension_schema(&self, _name: &str) -> Option<&str> {
        None
    }

    /// Effective membership, respecting a local removal.
    fn extension_membership(&self, member: &OwnershipIdentity) -> Option<&str> {
        self.extension_membership_history(member)
            .last()
            .filter(|change| change.is_add())
            .map(ExtensionMembershipChange::extension)
    }

    /// Membership history, including removals, in application order.
    fn extension_membership_history(
        &self,
        _member: &OwnershipIdentity,
    ) -> &[ExtensionMembershipChange] {
        &[]
    }
}
