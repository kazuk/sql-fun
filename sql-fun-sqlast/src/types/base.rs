//! Terminal named types.

use std::any::Any;
use crate::sem::FullName;
use super::TypeDefinition;

/// A terminal named type. Its resolved qualified name is its sole component.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct BaseType {
    name: FullName,
}

impl BaseType {
    /// Create a named terminal type.
    #[must_use]
    pub const fn new(name: FullName) -> Self {
        Self { name }
    }

    /// The resolved qualified name.
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }
}

impl TypeDefinition for BaseType {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
