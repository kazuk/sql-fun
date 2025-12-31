use crate::sem::FullName;

use super::{
    TypeDefinition, concrete_type::ConcreteTypeReference, dynamic_type::DynamicTypeReference,
};

/// type reference
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TypeReference {
    /// concrete type with array qualifies
    Concrete(ConcreteTypeReference),
    /// dynamic type
    Dynamic(DynamicTypeReference),
}

impl TypeReference {
    /// create a reference for dynamic type
    pub fn dynamic(type_def: &TypeDefinition) -> TypeReference {
        Self::Dynamic(DynamicTypeReference::new(type_def))
    }

    /// type full name
    #[track_caller]
    pub fn full_name(&self) -> &FullName {
        match self {
            Self::Concrete(c) => c.full_name(),
            Self::Dynamic(_) => todo!(),
        }
    }

    /// get type definition when owned
    pub fn type_definition(&self) -> Option<&TypeDefinition> {
        match self {
            Self::Concrete(_c) => None,
            Self::Dynamic(d) => Some(d.type_definition()),
        }
    }

    /// type is array?
    pub fn is_array(&self) -> Option<bool> {
        match self {
            Self::Concrete(c) => Some(c.is_array()),
            Self::Dynamic(d) => d.is_array(),
        }
    }

    /// get element type reference
    ///
    /// # Panics
    ///
    /// self requires array type
    ///
    pub fn element_type(&self) -> Self {
        match self {
            Self::Concrete(c) => c.element_type(),
            Self::Dynamic(_) => todo!(),
        }
    }

    /// is record type?
    pub fn is_record(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_record(),
            Self::Dynamic(_) => true,
        }
    }

    /// create a type reference to named type
    pub fn from_full_name(full_name: FullName) -> Self {
        Self::Concrete(ConcreteTypeReference::new(&full_name, false))
    }

    /// coverts type name with '_' as array
    pub fn from_master_type_name(name: &str) -> Self {
        Self::Concrete(ConcreteTypeReference::from_master_type_name(name))
    }

    /// create concrete type reference
    pub fn concrete_type_ref(full_name: &FullName, is_array: bool) -> Self {
        Self::Concrete(ConcreteTypeReference::new(full_name, is_array))
    }

    /// is multi range type?
    pub fn is_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// get subtype from range type
    pub fn range_subtype(&self) -> Self {
        match self {
            Self::Concrete(c) => c.range_subtype(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// is multi range type?
    pub fn is_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_multi_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// get base element type from multi range type
    pub fn multi_range_base_element(&self) -> TypeReference {
        match self {
            Self::Concrete(c) => c.multi_range_base_element(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is simple inference arg
    pub fn is_simple_infer_arg(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_simple_infer_arg(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is common inference arg
    pub fn is_common_infer_arg(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_common_infer_arg(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anyelement`
    pub fn is_any_element(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_element(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anyarrry`
    pub fn is_any_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anynonarray`
    pub fn is_any_non_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_non_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anyrange`
    pub fn is_any_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anymultirange`
    pub fn is_any_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_multi_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatible`
    pub fn is_any_compatible(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblearray`
    pub fn is_any_compatible_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblenonarray`
    pub fn is_any_compatible_non_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_non_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblerange`
    pub fn is_any_compatible_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblemultirange`
    pub fn is_any_compatible_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_multi_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// convert to range type
    pub fn into_range_type(&self) -> TypeReference {
        match self {
            Self::Concrete(c) => c.get_range_type(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// convert to multi range type
    pub fn into_multi_range_type(&self) -> TypeReference {
        match self {
            Self::Concrete(c) => c.get_multi_range_type(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// apply simple inference result
    pub fn apply_simple_infer_result(&mut self, infer_result_type: &TypeReference) {
        match self {
            Self::Concrete(c) => c.apply_simple_infer_result(infer_result_type),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// apply common inference result
    pub fn apply_common_infer_result(&mut self, infer_result_type: &TypeReference) {
        match self {
            Self::Concrete(c) => c.apply_common_infer_result(infer_result_type),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is dynamic
    pub fn is_dynamic(&self) -> bool {
        matches!(self, Self::Dynamic(_))
    }
}

impl std::fmt::Display for TypeReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Concrete(c) => write!(f, "{c}"),
            Self::Dynamic(_d) => todo!(),
        }
    }
}
