//! Loaded type values, kept independent of the legacy semantic type representation.
//!
//! This is an opt-in migration boundary. Existing semantic analysis and
//! `ParseContext` storage continue to use their original types.

pub mod array;
pub mod base;
mod imported;

use std::any::Any;

pub use self::imported::{
    CompositeType, DomainType, EnumType, ImplicitCompositeType, TableRowType,
};

/// A loaded type value, including types composed from other type values.
///
/// The category-specific components are owned by each implementation.
/// Identity comparison and inference behavior will be introduced once the
/// complete type classification has been specified.
pub trait TypeDefinition: std::fmt::Debug + Any {
    /// Enable inspection by a caller that knows the concrete type category.
    fn as_any(&self) -> &dyn Any;
}

/// Owned type value.
pub type TypeReference = Box<dyn TypeDefinition>;

/// Convert an existing semantic type reference without changing its owner.
///
/// A concrete array is represented by a structural array wrapper; a dynamic
/// reference recursively loads the enclosed legacy definition.
#[must_use]
pub fn from_legacy_reference(value: &crate::sem::TypeReference) -> TypeReference {
    use crate::sem::TypeReference as Legacy;
    match value {
        Legacy::Concrete(concrete) => {
            let base: TypeReference = Box::new(base::BaseType::new(concrete.full_name().clone()));
            if concrete.is_array() {
                Box::new(array::ArrayType::new(base))
            } else {
                base
            }
        }
        Legacy::Dynamic(dynamic) => from_legacy_definition(dynamic.type_definition()),
    }
}

/// Convert a definition obtained from an existing `ParseContext` lookup.
///
/// This is deliberately opt-in: the storage API and all existing consumers
/// retain their legacy types. Non-base definitions retain their complete
/// original payload as transitional loaded values.
#[must_use]
pub fn from_legacy_definition(value: &crate::sem::TypeDefinition) -> TypeReference {
    use crate::sem::TypeDefinition as Legacy;
    match value {
        Legacy::Builtin(reference) => from_legacy_reference(reference),
        Legacy::Enum(definition) => Box::new(EnumType::new(definition.clone())),
        Legacy::CompositeType(definition) => Box::new(CompositeType::new(definition.clone())),
        Legacy::Domain(definition) => Box::new(DomainType::new(definition.clone())),
        Legacy::TableRef(table, reference) => Box::new(TableRowType::new(
            table.clone(),
            from_legacy_reference(reference),
        )),
        Legacy::ImplicitComposite(columns) => {
            Box::new(ImplicitCompositeType::new(columns.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{array::ArrayType, base::BaseType, from_legacy_definition, from_legacy_reference};
    use crate::sem::{FullName, TypeDefinition, TypeReference};

    #[test]
    fn converts_named_base_type() {
        let name = FullName::with_catalog("db", "pg_catalog", "int4");
        let legacy = TypeDefinition::builtin(&name);
        let loaded = from_legacy_definition(&legacy);
        let base = loaded.as_any().downcast_ref::<BaseType>().unwrap();
        assert_eq!(base.name(), &name);
    }

    #[test]
    fn preserves_array_structure() {
        let name = FullName::with_schema("pg_catalog", "int4");
        let legacy = TypeReference::concrete_type_ref(&name, true);
        let loaded = from_legacy_reference(&legacy);
        let array = loaded.as_any().downcast_ref::<ArrayType>().unwrap();
        let base = array.element_type().as_any().downcast_ref::<BaseType>().unwrap();
        assert_eq!(base.name(), &name);
    }

    #[test]
    fn distinct_named_base_with_array_suffix_is_not_an_array() {
        let name = FullName::with_schema("public", "item[]");
        let legacy = TypeReference::concrete_type_ref(&name, false);
        let loaded = from_legacy_reference(&legacy);
        assert!(loaded.as_any().downcast_ref::<ArrayType>().is_none());
        assert_eq!(
            loaded.as_any().downcast_ref::<BaseType>().unwrap().name(),
            &name
        );
    }

    #[test]
    fn dynamic_builtin_is_converted_recursively() {
        let name = FullName::with_schema("pg_catalog", "text");
        let definition = TypeDefinition::builtin(&name);
        let dynamic = TypeReference::dynamic(&definition);
        let loaded = from_legacy_reference(&dynamic);
        assert_eq!(
            loaded.as_any().downcast_ref::<BaseType>().unwrap().name(),
            &name
        );
    }
}
