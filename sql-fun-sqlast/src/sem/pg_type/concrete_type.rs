use crate::{names::FullName, sem::TypeReference};

/// concrete type with array qualifies
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ConcreteTypeReference {
    base: FullName,
    is_array: bool,
}

impl std::fmt::Display for ConcreteTypeReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.base)?;
        if self.is_array {
            write!(f, "[]")?;
        }
        Ok(())
    }
}

impl ConcreteTypeReference {
    /// type full name
    pub const fn full_name(&self) -> &FullName {
        &self.base
    }

    /// is array type?
    pub const fn is_array(&self) -> bool {
        self.is_array
    }

    /// get element type reference
    ///
    /// # Panics
    ///
    /// self requires array type
    ///
    pub fn element_type(&self) -> TypeReference {
        assert!(self.is_array);
        TypeReference::Concrete(Self {
            base: self.base.clone(),
            is_array: false,
        })
    }

    /// is record type?
    pub fn is_record(&self) -> bool {
        self.base.is_record()
    }

    /// is range type?
    pub fn is_range(&self) -> bool {
        if self.is_array {
            false
        } else {
            self.base.is_range()
        }
    }

    /// coverts type name with '_' as array
    pub fn from_master_type_name(name: &str) -> Self {
        if let Some(name) = name.strip_prefix('_') {
            Self {
                base: FullName::with_schema("pg_catalog", name),
                is_array: true,
            }
        } else {
            Self {
                base: FullName::with_schema("pg_catalog", name),
                is_array: false,
            }
        }
    }

    /// create type reference
    pub fn new(full_name: &FullName, is_array: bool) -> Self {
        Self {
            base: full_name.clone(),
            is_array,
        }
    }

    /// get subtype from range type
    pub fn range_subtype(&self) -> TypeReference {
        TypeReference::concrete_type_ref(&self.base.range_subtype(), false)
    }

    /// is multi range type?
    pub fn is_multi_range(&self) -> bool {
        if self.is_array {
            false
        } else {
            self.base.is_multi_range()
        }
    }

    /// get base element type from multi range type
    pub fn multi_range_base_element(&self) -> TypeReference {
        TypeReference::concrete_type_ref(&self.base.multi_range_base_element(), false)
    }

    /// type is simple inference arg
    pub fn is_simple_infer_arg(&self) -> bool {
        if self.is_array {
            false
        } else {
            self.base.is_simple_infer_arg()
        }
    }

    /// type is common inference arg
    pub fn is_common_infer_arg(&self) -> bool {
        if self.is_array {
            false
        } else {
            self.base.is_common_infer_arg()
        }
    }

    /// type is just `anyelement`
    pub fn is_any_element(&self) -> bool {
        !self.is_array && self.base.is_any_element()
    }

    /// type is just `anyarrry`
    pub fn is_any_array(&self) -> bool {
        !self.is_array && self.base.is_any_array()
    }

    /// type is just `anynonarray`
    pub fn is_any_non_array(&self) -> bool {
        !self.is_array && self.base.is_any_non_array()
    }

    /// type is just `anyrange`
    pub fn is_any_range(&self) -> bool {
        !self.is_array && self.base.is_any_range()
    }

    /// type is just `anymultirange`
    pub fn is_any_multi_range(&self) -> bool {
        !self.is_array && self.base.is_any_multi_range()
    }

    /// type is just `anycompatible`
    pub fn is_any_compatible(&self) -> bool {
        !self.is_array && self.base.is_any_compatible()
    }

    /// type is just `anycompatiblearray`
    pub fn is_any_compatible_array(&self) -> bool {
        !self.is_array && self.base.is_any_compatible_array()
    }

    /// type is just `anycompatiblenonarray`
    pub fn is_any_compatible_non_array(&self) -> bool {
        !self.is_array && self.base.is_any_compatible_non_array()
    }

    /// type is just `anycompatiblerange`
    pub fn is_any_compatible_range(&self) -> bool {
        !self.is_array && self.base.is_any_compatible_range()
    }

    /// type is just `anycompatiblemultirange`
    pub fn is_any_compatible_multi_range(&self) -> bool {
        !self.is_array && self.base.is_any_compatible_multi_range()
    }

    /// convert to range type
    pub fn get_range_type(&self) -> TypeReference {
        todo!("range construction requires catalog semantics: #914")
    }

    /// convert to multi range type
    pub fn get_multi_range_type(&self) -> TypeReference {
        todo!("range construction requires catalog semantics: #914")
    }
}
