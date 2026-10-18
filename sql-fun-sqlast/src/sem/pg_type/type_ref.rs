use crate::sem::FullName;

use super::{
    TypeDefinition, concrete_type::ConcreteTypeReference, dynamic_type::DynamicTypeReference,
};

/// A concrete named type (optionally an array), or an inferred dynamic type.
///
/// Concrete types display their canonical [`FullName`] followed by `[]` for
/// an array. `FromStr` reads that canonical spelling without SQL case folding
/// or type-alias interpretation; dynamic types have no canonical text form.
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TypeReference {
    /// concrete type with array qualifies
    Concrete(ConcreteTypeReference),
    /// dynamic type
    Dynamic(DynamicTypeReference),
}

impl TypeReference {
    /// create a reference for dynamic type
    #[must_use]
    pub fn dynamic(type_def: &TypeDefinition) -> Self {
        Self::Dynamic(DynamicTypeReference::new(type_def))
    }

    /// type full name
    #[track_caller]
    #[must_use]
    pub fn full_name(&self) -> &FullName {
        match self {
            Self::Concrete(c) => c.full_name(),
            Self::Dynamic(_) => todo!(),
        }
    }

    /// get type definition when owned
    #[must_use]
    pub fn type_definition(&self) -> Option<&TypeDefinition> {
        match self {
            Self::Concrete(_c) => None,
            Self::Dynamic(d) => Some(d.type_definition()),
        }
    }

    /// type is array?
    #[must_use]
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
    #[must_use]
    pub fn element_type(&self) -> Self {
        match self {
            Self::Concrete(c) => c.element_type(),
            Self::Dynamic(_) => todo!(),
        }
    }

    /// is record type?
    #[must_use]
    pub fn is_record(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_record(),
            Self::Dynamic(_) => true,
        }
    }

    /// create a type reference to named type
    #[must_use]
    pub fn from_full_name(full_name: FullName) -> Self {
        Self::Concrete(ConcreteTypeReference::new(&full_name, false))
    }

    /// coverts type name with '_' as array
    #[must_use]
    pub fn from_master_type_name(name: &str) -> Self {
        Self::Concrete(ConcreteTypeReference::from_master_type_name(name))
    }

    /// create concrete type reference
    #[must_use]
    pub fn concrete_type_ref(full_name: &FullName, is_array: bool) -> Self {
        Self::Concrete(ConcreteTypeReference::new(full_name, is_array))
    }

    /// is multi range type?
    #[must_use]
    pub fn is_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// get subtype from range type
    #[must_use]
    pub fn range_subtype(&self) -> Self {
        match self {
            Self::Concrete(c) => c.range_subtype(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// is multi range type?
    #[must_use]
    pub fn is_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_multi_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// get base element type from multi range type
    #[must_use]
    pub fn multi_range_base_element(&self) -> Self {
        match self {
            Self::Concrete(c) => c.multi_range_base_element(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is simple inference arg
    #[must_use]
    pub fn is_simple_infer_arg(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_simple_infer_arg(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is common inference arg
    #[must_use]
    pub fn is_common_infer_arg(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_common_infer_arg(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anyelement`
    #[must_use]
    pub fn is_any_element(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_element(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anyarrry`
    #[must_use]
    pub fn is_any_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anynonarray`
    #[must_use]
    pub fn is_any_non_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_non_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anyrange`
    #[must_use]
    pub fn is_any_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anymultirange`
    #[must_use]
    pub fn is_any_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_multi_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatible`
    #[must_use]
    pub fn is_any_compatible(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblearray`
    #[must_use]
    pub fn is_any_compatible_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblenonarray`
    #[must_use]
    pub fn is_any_compatible_non_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_non_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblerange`
    #[must_use]
    pub fn is_any_compatible_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblemultirange`
    #[must_use]
    pub fn is_any_compatible_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_multi_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// convert to range type
    #[must_use]
    pub fn into_range_type(&self) -> Self {
        match self {
            Self::Concrete(c) => c.get_range_type(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// convert to multi range type
    #[must_use]
    pub fn into_multi_range_type(&self) -> Self {
        match self {
            Self::Concrete(c) => c.get_multi_range_type(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// apply simple inference result
    pub fn apply_simple_infer_result(&mut self, infer_result_type: &Self) {
        match self {
            Self::Concrete(c) => c.apply_simple_infer_result(infer_result_type),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// apply common inference result
    pub fn apply_common_infer_result(&mut self, infer_result_type: &Self) {
        match self {
            Self::Concrete(c) => c.apply_common_infer_result(infer_result_type),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is dynamic
    #[must_use]
    pub const fn is_dynamic(&self) -> bool {
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

impl std::str::FromStr for TypeReference {
    type Err = crate::sem::AnalysisError;

    /// Read canonical concrete type spelling, preserving quoted suffixes.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (name, is_array) = if let Some(name) = value.strip_suffix("[]") {
            (name, true)
        } else {
            (value, false)
        };
        Ok(Self::concrete_type_ref(
            &name.parse::<FullName>()?,
            is_array,
        ))
    }
}

#[cfg(test)]
mod canonical_type_tests {
    use super::TypeReference;
    use crate::sem::FullName;

    #[rstest::rstest]
    #[case::plain(FullName::with_schema("fixture", "item"))]
    #[case::dot(FullName::with_schema("a.b", "item"))]
    #[case::quote(FullName::with_schema("a", "b\"c"))]
    #[case::literal_array_suffix(FullName::with_schema("a", "item[]"))]
    #[case::catalog(FullName::with_catalog("db", "a.b", "c"))]
    fn preserves_type_identity(
        #[case] name: FullName,
        #[values(false, true)] array: bool,
    ) -> testresult::TestResult {
        let reference = TypeReference::concrete_type_ref(&name, array);
        assert_eq!(reference.to_string().parse::<TypeReference>()?, reference);
        assert_ne!(
            reference.to_string(),
            TypeReference::concrete_type_ref(&name, !array).to_string()
        );
        Ok(())
    }

    #[test]
    fn scalar_array_spelling_is_not_an_array() {
        let scalar = TypeReference::concrete_type_ref(&FullName::with_schema("a", "item[]"), false);
        let array = TypeReference::concrete_type_ref(&FullName::with_schema("a", "item"), true);
        assert_ne!(scalar.to_string(), array.to_string());
    }
}
