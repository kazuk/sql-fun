mod parse;

use crate::ast::{DeparseError, Dialect as AstDialect, render_full_name};

use std::fmt::Display;

use crate::names::{CatalogName, LocalName, SchemaName};

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

impl FullName {
    pub(crate) fn sqlite_identity(&self) -> Self {
        Self::new(
            &self
                .catalog()
                .map(|name| CatalogName::from(name.as_str().to_ascii_lowercase().as_str())),
            &self
                .schema()
                .map(|name| SchemaName::from(name.as_str().to_ascii_lowercase().as_str())),
            &LocalName::from(self.name().as_str().to_ascii_lowercase().as_str()),
        )
    }

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
    pub fn to_sql(&self, dialect: AstDialect) -> Result<String, DeparseError> {
        render_full_name(self, dialect)
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
}

#[cfg(test)]
mod construction_tests {
    use crate::names::{CatalogName, FullName, LocalName};
    #[test]
    #[should_panic(expected = "a catalog requires a schema")]
    fn constructor_prevents_catalog_only_names() {
        let _ = FullName::new(
            &Some(CatalogName::from("db")),
            &None,
            &LocalName::from("items"),
        );
    }
}

#[cfg(test)]
mod render_tests {
    use crate::{ast::Dialect, names::FullName};
    #[rstest::rstest]
    #[case::empty(FullName::from_local_name(""))]
    #[case::empty_schema(FullName::with_schema("", "x"))]
    #[case::nul(FullName::from_local_name("a\0b"))]
    fn checked_postgres_emission_rejects_invalid_sql(#[case] name: FullName) {
        assert!(
            name.to_sql(Dialect::postgres())
                .unwrap_err()
                .is_invalid_syntax()
        );
    }

    #[test]
    fn checked_sqlite_emission_rejects_catalog() {
        assert!(
            FullName::with_catalog("db", "main", "x")
                .to_sql(Dialect::sqlite())
                .is_err()
        );
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
mod display_tests {
    use crate::names::FullName;
    #[rstest::rstest]
    #[case::plain("simple_name", "simple_name")]
    #[case::dot("a.b", r#""a.b""#)]
    #[case::quote("a\"b", r#""a""b""#)]
    #[case::mixed("Mixed", r#""Mixed""#)]
    #[case::space("odd name", r#""odd name""#)]
    #[case::reserved("select", r#""select""#)]
    #[case::unicode("éclair", r#""éclair""#)]
    #[case::empty("", r#""""#)]
    #[case::numeric("12abc", r#""12abc""#)]
    #[case::array_spelling("item[]", r#""item[]""#)]
    fn formats_each_component_identically(#[case] component: &str, #[case] expected: &str) {
        assert_eq!(FullName::from_local_name(component).to_string(), expected);
        assert_eq!(
            FullName::with_schema(component, component).to_string(),
            format!("{expected}.{expected}")
        );
        assert_eq!(
            FullName::with_catalog(component, component, component).to_string(),
            format!("{expected}.{expected}.{expected}")
        );
    }
}

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
    use crate::names::FullName;

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
