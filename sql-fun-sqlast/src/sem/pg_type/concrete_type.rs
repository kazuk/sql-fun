use crate::sem::{FullName, TypeReference};

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

    /// apply simple inference result
    pub fn apply_simple_infer_result(&mut self, infer_result_type: &TypeReference) {
        if self.is_any_element() {
            self.base = infer_result_type.full_name().clone();
            self.is_array = infer_result_type.is_array().unwrap_or(false);
        } else if self.is_any_array() {
            self.base = infer_result_type.full_name().clone();
            self.is_array = true;
        } else if self.is_any_non_array() {
            self.base = infer_result_type.full_name().clone();
            self.is_array = false;
        }
    }

    /// apply common inference result
    pub fn apply_common_infer_result(&mut self, infer_result_type: &TypeReference) {
        if self.is_any_compatible() {
            self.base = infer_result_type.full_name().clone();
            self.is_array = infer_result_type.is_array().unwrap_or(false);
        } else if self.is_any_compatible_array() {
            self.base = infer_result_type.full_name().clone();
            self.is_array = true;
        } else if self.is_any_compatible_non_array() {
            self.base = infer_result_type.full_name().clone();
            self.is_array = false;
        }
    }
}
