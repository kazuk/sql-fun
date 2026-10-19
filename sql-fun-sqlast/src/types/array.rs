//! Structural array types.

use std::any::Any;
use super::{TypeDefinition, TypeReference};

/// An array type containing its element type as a loaded value.
#[derive(Debug)]
pub struct ArrayType {
    element_type: TypeReference,
}

impl ArrayType {
    /// Wrap an element type in an array type.
    #[must_use]
    pub fn new(element_type: TypeReference) -> Self {
        Self { element_type }
    }

    /// Access the element type without consulting catalog storage.
    #[must_use]
    pub fn element_type(&self) -> &dyn TypeDefinition {
        self.element_type.as_ref()
    }
}

impl TypeDefinition for ArrayType {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
