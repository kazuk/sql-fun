//! A terminal named type whose resolved name determines identity.

use crate::sem::FullName;
use crate::types::{TypeDefinition, TypeReference};

use std::any::Any;

/// A terminal named type whose resolved name determines identity.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BaseType {
    name: FullName,
}

impl BaseType {
    /// Create a named terminal type.
    #[must_use]
    pub fn new(name: &FullName) -> Self {
        Self { name: name.clone() }
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

    fn clone_box(&self) -> TypeReference {
        Box::new(self.clone())
    }

    fn same_type(&self, other: &dyn TypeDefinition) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::BaseType;
    use crate::sem::FullName;
    use crate::types::TypeDefinition;

    #[rstest::rstest]
    #[case::same(FullName::with_catalog("db", "public", "item"), true)]
    #[case::catalog(FullName::with_catalog("other", "public", "item"), false)]
    #[case::schema(FullName::with_catalog("db", "other", "item"), false)]
    #[case::local(FullName::with_catalog("db", "public", "other"), false)]
    #[case::qualification(FullName::with_schema("public", "item"), false)]
    fn identity_uses_complete_name(#[case] other: FullName, #[case] expected: bool) {
        let name = FullName::with_catalog("db", "public", "item");
        let base = BaseType::new(&name);
        assert_eq!(base.name(), &name);
        assert_eq!(base.same_type(&BaseType::new(&other)), expected);
        assert!(base.same_type(base.clone_box().as_ref()));
    }
}
