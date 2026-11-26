use crate::sem::{OperatorClassDefinition, OperatorFamily, OperatorFamilyKey, TypeReference};

/// Read access to the effective operator family catalog.
pub trait OperatorFamilyInfoRead {
    /// Definitions of operator classes in the effective catalog.
    fn operator_classes(&self) -> Vec<&OperatorClassDefinition> {
        Vec::new()
    }

    /// Class selected for member ownership in this family and exact input type.
    fn operator_class_for_family_type(
        &self,
        _key: &OperatorFamilyKey,
        _typ: &TypeReference,
    ) -> Option<&OperatorClassDefinition> {
        None
    }

    /// Whether the effective catalog has a default class for the method and input type.
    fn has_default_operator_class(&self, _method: &str, _typ: &TypeReference) -> bool {
        false
    }

    /// Read a family by exact identity.
    fn operator_family(&self, _key: &OperatorFamilyKey) -> Option<&OperatorFamily> {
        None
    }
}

/// Write access to the current schema layer's operator family catalog.
pub trait OperatorFamilyInfoWrite: Sized {
    /// Publish a validated class and its associated family snapshot together.
    fn publish_operator_class(
        self,
        class: &OperatorClassDefinition,
        family: &OperatorFamily,
    ) -> Self;

    /// Publish a validated complete family snapshot.
    fn publish_operator_family(self, family: &OperatorFamily) -> Self;
}
