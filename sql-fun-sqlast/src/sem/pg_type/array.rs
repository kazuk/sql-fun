use crate::sem::TypeReference;

/// A structural array with a fully retained element type.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArrayTypeDefinition {
    element: TypeReference,
}

impl ArrayTypeDefinition {
    pub(crate) fn new(element: &TypeReference) -> Self {
        Self {
            element: element.clone(),
        }
    }

    /// The element's identity, including anonymous record fields.
    #[must_use]
    pub const fn element(&self) -> &TypeReference {
        &self.element
    }
}
