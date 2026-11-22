use crate::sem::{OperatorFamily, OperatorFamilyKey};

/// Read access to the effective operator family catalog.
pub trait OperatorFamilyInfoRead {
    /// Read a family by exact identity.
    fn operator_family(&self, _key: &OperatorFamilyKey) -> Option<&OperatorFamily> {
        None
    }
}

/// Write access to the current schema layer's operator family catalog.
pub trait OperatorFamilyInfoWrite: Sized {
    /// Publish a validated complete family snapshot.
    fn publish_operator_family(self, family: &OperatorFamily) -> Self;
}
