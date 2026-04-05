mod impl_coversion_traits;

use std::fmt::Display;

use crate::sem::{CatalogName, LocalName, SchemaName};

/// full name parts.
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FullName {
    catalog: Option<CatalogName>,
    schema: Option<SchemaName>,
    name: LocalName,
}

impl FullName {
    /// create instance
    #[must_use]
    pub fn new(
        catalog: &Option<CatalogName>,
        schema: &Option<SchemaName>,
        name: &LocalName,
    ) -> Self {
        Self {
            catalog: catalog.clone(),
            schema: schema.clone(),
            name: name.clone(),
        }
    }

    /// get `local_name` part as String
    #[must_use]
    pub fn local_name(&self) -> String {
        self.name.to_string()
    }

    /// get catalog part
    #[must_use]
    pub fn catalog(&self) -> Option<&CatalogName> {
        self.catalog.as_ref()
    }

    /// get schema part
    #[must_use]
    pub fn schema(&self) -> Option<&SchemaName> {
        self.schema.as_ref()
    }

    /// get local-name as [`LocalName`]
    #[must_use]
    pub fn name(&self) -> &LocalName {
        &self.name
    }

    /// create with catalog
    pub fn with_catalog(catalog: &str, schema: Option<&str>, name: &str) -> Self {
        Self {
            catalog: Some(CatalogName::from(catalog)),
            schema: schema.map(SchemaName::from),
            name: LocalName::from(name),
        }
    }

    /// create with schema
    #[must_use]
    pub fn with_schema(schema: &str, local: &str) -> Self {
        Self {
            catalog: None,
            schema: Some(SchemaName::from(schema)),
            name: LocalName::from(local),
        }
    }

    /// set schema
    #[must_use]
    pub fn set_schema(mut self, schema: SchemaName) -> Self {
        self.schema = Some(schema);
        self
    }

    /// returns schema replaced full name
    #[must_use]
    pub fn replace_schema(&self, schema: &SchemaName) -> Self {
        self.clone().set_schema(schema.clone())
    }

    /// supply `catalog_name`
    #[must_use]
    pub fn catalog_default(&self, catalog_name: &str) -> Self {
        if self.catalog().is_none() && self.schema().is_none() {
            Self::with_catalog(catalog_name, None, self.name().as_str())
        } else {
            self.clone()
        }
    }

    /// create `FullName` from only local part
    #[must_use]
    pub fn from_local_name(local_name: &str) -> Self {
        Self {
            catalog: None,
            schema: None,
            name: LocalName::from(local_name),
        }
    }

    /// builtin record type
    #[must_use]
    pub fn is_record(&self) -> bool {
        self.schema
            .as_ref()
            .is_some_and(super::schema::SchemaName::is_pg_catalog)
            && self.name.is_record()
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
        self.schema
            .as_ref()
            .is_some_and(super::schema::SchemaName::is_pg_catalog)
    }

    /// just anyarray type
    #[must_use]
    pub fn is_any_array(&self) -> bool {
        self.schema_is_pg_catalog() && self.name == LocalName::from("anyarray")
    }

    /// just anyelement type
    #[must_use]
    pub fn is_any_element(&self) -> bool {
        self.schema_is_pg_catalog() && self.name == LocalName::from("anyelement")
    }

    /// just anyrange type
    #[must_use]
    pub fn is_any_range(&self) -> bool {
        self.schema_is_pg_catalog() && self.name == LocalName::from("anyrange")
    }

    /// just anymultirange
    #[must_use]
    pub fn is_any_multi_range(&self) -> bool {
        self.schema_is_pg_catalog() && self.name == LocalName::from("anymultirange")
    }

    /// just anynonarray
    #[must_use]
    pub fn is_any_non_array(&self) -> bool {
        self.schema_is_pg_catalog() && self.name == LocalName::from("anynonarray")
    }

    /// just anycompatible
    #[must_use]
    pub fn is_any_compatible(&self) -> bool {
        self.schema_is_pg_catalog() && self.name == LocalName::from("anycompatible")
    }

    /// just anycompatiblearray
    #[must_use]
    pub fn is_any_compatible_array(&self) -> bool {
        self.schema_is_pg_catalog() && self.name == LocalName::from("anycompatiblearray")
    }

    /// just anycompatiblenonarray
    #[must_use]
    pub fn is_any_compatible_non_array(&self) -> bool {
        self.schema_is_pg_catalog() && self.name == LocalName::from("anycompatiblenonarray")
    }

    /// just anycompatiblerange
    #[must_use]
    pub fn is_any_compatible_range(&self) -> bool {
        self.schema_is_pg_catalog() && self.name == LocalName::from("anycompatiblerange")
    }

    /// just anycompatiblemultirange
    #[must_use]
    pub fn is_any_compatible_multi_range(&self) -> bool {
        self.schema_is_pg_catalog() && self.name == LocalName::from("anycompatiblemultirange")
    }
}

impl Display for FullName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(catalog) = &self.catalog {
            write!(f, "{catalog}.")?;
        }

        if let Some(schema) = &self.schema {
            write!(f, "{schema}.")?;
        }

        write!(f, "{}", self.name)
    }
}
