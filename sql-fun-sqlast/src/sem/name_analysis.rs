//! Semantic operations on shared SQL identities.
use crate::names::{FullName, LocalName, SchemaName};

impl FullName {
    /// builtin record type
    #[must_use]
    pub fn is_record(&self) -> bool {
        self.schema().is_some_and(SchemaName::is_pg_catalog) && self.name().is_record()
    }

    /// is range type?
    #[must_use]
    pub fn is_range(&self) -> bool {
        !self.is_multi_range() && self.local_name().as_str().ends_with("range")
    }

    /// get subtype from range type
    #[must_use]
    pub fn range_subtype(&self) -> Self {
        todo!()
    }

    /// is multi range type?
    #[must_use]
    pub fn is_multi_range(&self) -> bool {
        self.local_name().as_str().ends_with("multirange")
    }

    /// get base element type from multi range type
    #[must_use]
    pub fn multi_range_base_element(&self) -> Self {
        todo!()
    }

    /// is simple infrence types
    #[must_use]
    pub fn is_simple_infer_arg(&self) -> bool {
        self.is_any_array()
            || self.is_any_element()
            || self.is_any_range()
            || self.is_any_multi_range()
            || self.is_any_non_array()
    }

    /// is common inference types
    #[must_use]
    pub fn is_common_infer_arg(&self) -> bool {
        self.is_any_compatible()
            || self.is_any_compatible_array()
            || self.is_any_compatible_non_array()
            || self.is_any_compatible_range()
            || self.is_any_compatible_multi_range()
    }

    fn schema_is_pg_catalog(&self) -> bool {
        self.schema().is_some_and(SchemaName::is_pg_catalog)
    }

    /// just anyarray type
    #[must_use]
    pub fn is_any_array(&self) -> bool {
        self.schema_is_pg_catalog() && *self.name() == LocalName::from("anyarray")
    }

    /// just anyelement type
    #[must_use]
    pub fn is_any_element(&self) -> bool {
        self.schema_is_pg_catalog() && *self.name() == LocalName::from("anyelement")
    }

    /// just anyrange type
    #[must_use]
    pub fn is_any_range(&self) -> bool {
        self.schema_is_pg_catalog() && *self.name() == LocalName::from("anyrange")
    }

    /// just anymultirange
    #[must_use]
    pub fn is_any_multi_range(&self) -> bool {
        self.schema_is_pg_catalog() && *self.name() == LocalName::from("anymultirange")
    }

    /// just anynonarray
    #[must_use]
    pub fn is_any_non_array(&self) -> bool {
        self.schema_is_pg_catalog() && *self.name() == LocalName::from("anynonarray")
    }

    /// just anycompatible
    #[must_use]
    pub fn is_any_compatible(&self) -> bool {
        self.schema_is_pg_catalog() && *self.name() == LocalName::from("anycompatible")
    }

    /// just anycompatiblearray
    #[must_use]
    pub fn is_any_compatible_array(&self) -> bool {
        self.schema_is_pg_catalog() && *self.name() == LocalName::from("anycompatiblearray")
    }

    /// just anycompatiblenonarray
    #[must_use]
    pub fn is_any_compatible_non_array(&self) -> bool {
        self.schema_is_pg_catalog() && *self.name() == LocalName::from("anycompatiblenonarray")
    }

    /// just anycompatiblerange
    #[must_use]
    pub fn is_any_compatible_range(&self) -> bool {
        self.schema_is_pg_catalog() && *self.name() == LocalName::from("anycompatiblerange")
    }

    /// just anycompatiblemultirange
    #[must_use]
    pub fn is_any_compatible_multi_range(&self) -> bool {
        self.schema_is_pg_catalog() && *self.name() == LocalName::from("anycompatiblemultirange")
    }
}
