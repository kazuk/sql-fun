mod impl_conversion_traits;
mod parse;

use std::fmt::Display;

#[cfg(feature = "sqlite")]
use crate::sem::AnalysisError;
use crate::sem::{CatalogName, LocalName, SchemaName};

/// A logical SQL name with one, two, or three components.
///
/// A catalog requires a schema. Components contain logical identifier values,
/// without SQL delimiters; constructors do not perform SQL case folding.
/// `Display` is an injective canonical spelling used by name-search tries. It
/// quotes punctuation, case-sensitive spellings and PostgreSQL keywords where
/// necessary. It is not dialect validation: empty identifiers, for example,
/// are representable here but are not legal PostgreSQL SQL.
///
/// `FromStr` reads this component spelling (including doubled double quotes),
/// preserving case, without interpreting SQL type aliases or folding case.
/// Use the dialect's SQL parser when reading SQL, and [`Self::to_sql`] when
/// emitting a name for a selected dialect.
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize)]
pub struct FullName {
    catalog: Option<CatalogName>,
    schema: Option<SchemaName>,
    name: LocalName,
}

#[cfg(feature = "sqlite")]
impl TryFrom<&crate::syn::sqlite::QualifiedName> for FullName {
    type Error = AnalysisError;

    fn try_from(value: &crate::syn::sqlite::QualifiedName) -> Result<Self, Self::Error> {
        Ok(Self {
            catalog: None,
            schema: value.db_name().map(SchemaName::try_from).transpose()?,
            name: LocalName::try_from(value.name())?,
        })
    }
}

impl FullName {
    pub(crate) fn sqlite_eq(&self, other: &Self) -> bool {
        const fn equal(left: Option<&str>, right: Option<&str>) -> bool {
            match (left, right) {
                (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
                (None, None) => true,
                _ => false,
            }
        }
        equal(
            self.catalog().map(CatalogName::as_str),
            other.catalog().map(CatalogName::as_str),
        ) && equal(
            self.schema().map(SchemaName::as_str),
            other.schema().map(SchemaName::as_str),
        ) && self
            .name()
            .as_str()
            .eq_ignore_ascii_case(other.name().as_str())
    }

    /// Construct from logical components.
    ///
    /// # Panics
    /// Panics if a catalog is supplied without a schema.
    #[must_use]
    pub fn new(
        catalog: &Option<CatalogName>,
        schema: &Option<SchemaName>,
        name: &LocalName,
    ) -> Self {
        assert!(
            catalog.is_none() || schema.is_some(),
            "a catalog requires a schema"
        );
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
    pub const fn catalog(&self) -> Option<&CatalogName> {
        self.catalog.as_ref()
    }

    /// get schema part
    #[must_use]
    pub const fn schema(&self) -> Option<&SchemaName> {
        self.schema.as_ref()
    }

    /// get local-name as [`LocalName`]
    #[must_use]
    pub const fn name(&self) -> &LocalName {
        &self.name
    }

    /// Construct a three-component name. A catalog always requires a schema.
    #[must_use]
    pub fn with_catalog(catalog: &str, schema: &str, name: &str) -> Self {
        Self::new(
            &Some(CatalogName::from(catalog)),
            &Some(SchemaName::from(schema)),
            &LocalName::from(name),
        )
    }

    /// Render a checked SQL name for an explicitly selected dialect.
    ///
    /// All components are quoted. PostgreSQL rejects empty identifiers; both
    /// dialects reject NUL. SQLite rejects three-component relation names.
    /// This does not verify that a catalog exists or is accessible.
    ///
    /// # Errors
    /// Returns an error if the name cannot be emitted for the selected dialect.
    pub fn to_sql(&self, dialect: crate::ast::Dialect) -> Result<String, crate::ast::DeparseError> {
        crate::ast::render_full_name(self, dialect)
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

// Quoted components escape every quote and protect every separator. Unquoted
// components contain neither. With catalog-only states excluded, the component
// count uniquely identifies the qualification shape, so this spelling is injective.
impl Display for FullName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(catalog) = &self.catalog {
            write!(
                f,
                "{}.",
                crate::identifier::CanonicalIdentifier::from(catalog.as_str())
            )?;
        }

        if let Some(schema) = &self.schema {
            write!(
                f,
                "{}.",
                crate::identifier::CanonicalIdentifier::from(schema.as_str())
            )?;
        }

        write!(
            f,
            "{}",
            crate::identifier::CanonicalIdentifier::from(self.name.as_str())
        )
    }
}

#[cfg(test)]
mod tests;

impl<'de> serde::Deserialize<'de> for FullName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(rename = "FullName")]
        struct Components {
            catalog: Option<CatalogName>,
            schema: Option<SchemaName>,
            name: LocalName,
        }
        let parts = <Components as serde::Deserialize>::deserialize(deserializer)?;
        if parts.catalog.is_some() && parts.schema.is_none() {
            return Err(serde::de::Error::custom("a catalog requires a schema"));
        }
        Ok(Self::new(&parts.catalog, &parts.schema, &parts.name))
    }
}

#[cfg(test)]
mod deserialize_tests {
    use super::FullName;

    #[test]
    fn rejects_catalog_without_schema() {
        let input = r#"{"catalog":"db","schema":null,"name":"items"}"#;
        let error = serde_json::from_str::<FullName>(input).unwrap_err();
        assert!(error.to_string().contains("a catalog requires a schema"));
    }

    #[rstest::rstest]
    #[case::local(FullName::from_local_name("a.b"))]
    #[case::schema(FullName::with_schema("", "Mixed"))]
    #[case::catalog(FullName::with_catalog("my.db", "a.b", "c.d"))]
    fn preserves_serialized_shape_and_valid_identity(
        #[case] name: FullName,
    ) -> testresult::TestResult {
        let encoded = serde_json::to_value(&name)?;
        assert_eq!(encoded["catalog"], serde_json::to_value(name.catalog())?);
        assert_eq!(encoded["schema"], serde_json::to_value(name.schema())?);
        assert_eq!(encoded["name"], name.name().as_str());
        assert_eq!(serde_json::from_value::<FullName>(encoded)?, name);
        Ok(())
    }
}
