//! Named domain types with loaded base types.

use super::{TypeDefinition, TypeReference};
use crate::sem::FullName;
use std::any::Any;

/// A named domain and the loaded type on which it is based.
#[derive(Debug, Clone, Eq)]
pub struct DomainType {
    name: FullName,
    base_type: TypeReference,
}

impl PartialEq for DomainType {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.base_type.same_type(other.base_type.as_ref())
    }
}

impl DomainType {
    /// Create a domain by cloning its resolved name and loaded base type.
    #[must_use]
    pub fn new(name: &FullName, base_type: &dyn TypeDefinition) -> Self {
        Self {
            name: name.clone(),
            base_type: base_type.clone_box(),
        }
    }

    /// The resolved qualified domain name.
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }

    /// The loaded base type, which may itself be a domain or array.
    #[must_use]
    pub fn base_type(&self) -> &dyn TypeDefinition {
        self.base_type.as_ref()
    }
}

#[cfg(test)]
mod construction_tests {
    use super::DomainType;
    use crate::sem::FullName;
    use crate::types::{array::ArrayType, base::BaseType};

    #[test]
    fn owns_its_resolved_name_and_loaded_array_base() {
        let name = FullName::with_schema("public", "integer_list");
        let base = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
        let array = ArrayType::new(&base);
        let domain = DomainType::new(&name, &array);

        assert_eq!(domain.name(), &name);
        assert!(domain.base_type().same_type(&array));
        assert!(domain.base_type().as_any().is::<ArrayType>());
    }
}

impl TypeDefinition for DomainType {
    fn contains_polymorphic(&self) -> bool {
        self.base_type.contains_polymorphic()
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
mod identity_tests {
    use super::DomainType;
    use crate::sem::FullName;
    use crate::types::{TypeDefinition, array::ArrayType, base::BaseType};

    #[test]
    fn cloned_domain_preserves_recursive_identity() {
        let base = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
        let inner = DomainType::new(&FullName::with_schema("public", "positive"), &base);
        let domain = DomainType::new(&FullName::with_schema("public", "count"), &inner);
        let cloned = domain.clone_box();

        assert!(domain.same_type(cloned.as_ref()));
        assert!(cloned.same_type(&domain));
        assert!(domain.base_type().same_type(&inner));
    }

    #[rstest::rstest]
    #[case::different_domain_name("other", "int4")]
    #[case::different_base_type("count", "int8")]
    fn identity_requires_name_and_loaded_base(
        #[case] other_name: &str,
        #[case] other_base_name: &str,
    ) {
        let base = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
        let domain = DomainType::new(&FullName::with_schema("public", "count"), &base);
        let other_base = BaseType::new(&FullName::with_schema("pg_catalog", other_base_name));
        let other = DomainType::new(&FullName::with_schema("public", other_name), &other_base);

        assert!(!domain.same_type(&other));
        assert!(!other.same_type(&domain));
        assert!(!domain.same_type(&base));
        assert!(!base.same_type(&domain));
    }

    #[test]
    fn domain_over_array_differs_from_array_of_domain() {
        let name = FullName::with_schema("public", "count");
        let base = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
        let array = ArrayType::new(&base);
        let domain_over_array = DomainType::new(&name, &array);
        let scalar_domain = DomainType::new(&name, &base);
        let array_of_domain = ArrayType::new(&scalar_domain);

        assert!(!domain_over_array.same_type(&array_of_domain));
        assert!(!array_of_domain.same_type(&domain_over_array));
        assert!(!domain_over_array.same_type(&scalar_domain));
    }
}
