//! A structural array type owning its loaded element type.

use crate::types::{TypeDefinition, TypeReference};

use std::any::Any;

/// A structural array type owning its loaded element type.
#[derive(Debug, Clone, Eq)]
pub struct ArrayType {
    element_type: TypeReference,
}

impl ArrayType {
    /// Wrap a clone of an element type in an array type.
    #[must_use]
    pub fn new(element_type: &dyn TypeDefinition) -> Self {
        Self {
            element_type: element_type.clone_box(),
        }
    }

    /// Access the owned element type without consulting catalog storage.
    #[must_use]
    pub fn element_type(&self) -> &dyn TypeDefinition {
        self.element_type.as_ref()
    }
}

impl PartialEq for ArrayType {
    fn eq(&self, other: &Self) -> bool {
        self.element_type.same_type(other.element_type.as_ref())
    }
}

impl TypeDefinition for ArrayType {
    fn contains_polymorphic(&self) -> bool {
        self.element_type.contains_polymorphic()
    }

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
    use super::ArrayType;
    use crate::sem::FullName;
    use crate::types::base::BaseType;
    use crate::types::{TypeDefinition, TypeReference};

    #[rstest::rstest]
    #[case::same("int4", false, true)]
    #[case::different_element("text", false, false)]
    #[case::nested("int4", true, false)]
    fn identity_uses_element_structure(
        #[case] element_name: &str,
        #[case] nested: bool,
        #[case] expected: bool,
    ) {
        let base = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
        let array = ArrayType::new(&base);
        let other_base = BaseType::new(&FullName::with_schema("pg_catalog", element_name));
        let other_element: TypeReference = if nested {
            Box::new(ArrayType::new(&other_base))
        } else {
            Box::new(other_base)
        };
        let other = ArrayType::new(other_element.as_ref());
        assert_eq!(array.same_type(&other), expected);
        assert!(array.element_type().same_type(&base));
        assert!(array.same_type(array.clone_box().as_ref()));
        assert!(!array.same_type(&base));
    }

    #[test]
    fn owns_element_after_source_is_dropped() {
        let array = {
            let base = BaseType::new(&FullName::from_local_name("item[]"));
            ArrayType::new(&base)
        };
        let base = BaseType::new(&FullName::from_local_name("item[]"));
        assert!(array.element_type().same_type(&base));
        assert!(!array.same_type(&base));
    }
}
