use crate::{ast::Dialect, context::BaseParseContext, names::FullName, sem::CatalogNameLookup};

/// Qualified collation identity, including encoding eligibility.
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
pub struct CollationIdentity {
    schema: String,
    name: String,
    encoding: i32,
}

impl CollationIdentity {
    /// Construct an exact qualified identity with PostgreSQL encoding applicability.
    pub fn new(name: &FullName, encoding: i32) -> Option<Self> {
        if name.catalog().is_some() || !(-1..=34).contains(&encoding) {
            return None;
        }
        Some(Self {
            schema: name.schema()?.as_str().to_owned(),
            name: name.name().as_str().to_owned(),
            encoding,
        })
    }

    /// Retained `schema` catalog property.
    pub fn schema(&self) -> &str {
        &self.schema
    }
    /// Retained `name` catalog property.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Retained `encoding` catalog property.
    pub const fn encoding(&self) -> i32 {
        self.encoding
    }
    /// Exact schema-qualified catalog name, without display-string parsing.
    pub fn qualified_name(&self) -> FullName {
        FullName::with_schema(&self.schema, &self.name)
    }
}

/// Exact catalog type identity, including types outside the extraction scope.
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
pub struct CatalogTypeIdentity {
    schema: String,
    name: String,
}

impl CatalogTypeIdentity {
    /// Retained `schema` catalog property.
    pub fn schema(&self) -> &str {
        &self.schema
    }
    /// Retained `name` catalog property.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Exact schema-qualified catalog name, without display-string parsing.
    pub fn qualified_name(&self) -> FullName {
        FullName::with_schema(&self.schema, &self.name)
    }
}

/// Operator-family identity scoped to its access method.
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
pub struct OperatorFamilyIdentity {
    schema: String,
    name: String,
    access_method: String,
}

impl OperatorFamilyIdentity {
    /// Retained `schema` catalog property.
    pub fn schema(&self) -> &str {
        &self.schema
    }
    /// Retained `name` catalog property.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Retained `access_method` catalog property.
    pub fn access_method(&self) -> &str {
        &self.access_method
    }
    /// Exact schema-qualified catalog name, without display-string parsing.
    pub fn qualified_name(&self) -> FullName {
        FullName::with_schema(&self.schema, &self.name)
    }
}

/// Extracted collation properties; nullable catalog values remain nullable.
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
pub struct CollationDefinition {
    schema: String,
    name: String,
    encoding: i32,
    provider: String,
    deterministic: bool,
    collate: Option<String>,
    ctype: Option<String>,
    locale: Option<String>,
    icu_rules: Option<String>,
    version: Option<String>,
}

impl CollationDefinition {
    pub(crate) fn matches_name(&self, name: &FullName) -> bool {
        name.catalog().is_none()
            && name
                .schema()
                .is_some_and(|schema| schema.as_str() == self.schema)
            && name.name().as_str() == self.name
    }

    /// Retained `schema` catalog property.
    pub fn schema(&self) -> &str {
        &self.schema
    }
    /// Retained `name` catalog property.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Retained `encoding` catalog property.
    pub const fn encoding(&self) -> i32 {
        self.encoding
    }
    /// Retained `provider` catalog property.
    pub fn provider(&self) -> &str {
        &self.provider
    }
    /// Retained `deterministic` catalog property.
    pub const fn deterministic(&self) -> bool {
        self.deterministic
    }
    /// Retained `collate` catalog property.
    pub const fn collate(&self) -> &Option<String> {
        &self.collate
    }
    /// Retained `ctype` catalog property.
    pub const fn ctype(&self) -> &Option<String> {
        &self.ctype
    }
    /// Retained `locale` catalog property.
    pub const fn locale(&self) -> &Option<String> {
        &self.locale
    }
    /// Retained `icu_rules` catalog property.
    pub const fn icu_rules(&self) -> &Option<String> {
        &self.icu_rules
    }
    /// Retained `version` catalog property.
    pub const fn version(&self) -> &Option<String> {
        &self.version
    }
    /// Exact schema-qualified catalog name, without display-string parsing.
    pub fn qualified_name(&self) -> FullName {
        FullName::with_schema(&self.schema, &self.name)
    }
}

/// Source database encoding and default-collation provenance.
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
pub struct DatabaseCollation {
    encoding: i32,
    encoding_name: String,
    provider: String,
    collate: Option<String>,
    ctype: Option<String>,
    locale: Option<String>,
    icu_rules: Option<String>,
    version: Option<String>,
    default_collation: CollationIdentity,
}

impl DatabaseCollation {
    /// Retained `encoding` catalog property.
    pub const fn encoding(&self) -> i32 {
        self.encoding
    }
    /// Retained `encoding_name` catalog property.
    pub fn encoding_name(&self) -> &str {
        &self.encoding_name
    }
    /// Retained `provider` catalog property.
    pub fn provider(&self) -> &str {
        &self.provider
    }
    /// Retained `collate` catalog property.
    pub const fn collate(&self) -> &Option<String> {
        &self.collate
    }
    /// Retained `ctype` catalog property.
    pub const fn ctype(&self) -> &Option<String> {
        &self.ctype
    }
    /// Retained `locale` catalog property.
    pub const fn locale(&self) -> &Option<String> {
        &self.locale
    }
    /// Retained `icu_rules` catalog property.
    pub const fn icu_rules(&self) -> &Option<String> {
        &self.icu_rules
    }
    /// Retained `version` catalog property.
    pub const fn version(&self) -> &Option<String> {
        &self.version
    }
    /// Retained `default_collation` catalog property.
    pub const fn default_collation(&self) -> &CollationIdentity {
        &self.default_collation
    }
}

/// Extracted operator class and its family/type/default relationships.
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
pub struct OperatorClassDefinition {
    schema: String,
    name: String,
    access_method: String,
    family: OperatorFamilyIdentity,
    input_type: CatalogTypeIdentity,
    default: bool,
    key_type: Option<CatalogTypeIdentity>,
}

impl OperatorClassDefinition {
    pub(crate) fn matches_name(&self, name: &FullName) -> bool {
        name.catalog().is_none()
            && name
                .schema()
                .is_some_and(|schema| schema.as_str() == self.schema)
            && name.name().as_str() == self.name
    }

    /// Retained `schema` catalog property.
    pub fn schema(&self) -> &str {
        &self.schema
    }
    /// Retained `name` catalog property.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Retained `access_method` catalog property.
    pub fn access_method(&self) -> &str {
        &self.access_method
    }
    /// Retained `family` catalog property.
    pub const fn family(&self) -> &OperatorFamilyIdentity {
        &self.family
    }
    /// Retained `input_type` catalog property.
    pub const fn input_type(&self) -> &CatalogTypeIdentity {
        &self.input_type
    }
    /// Retained `default` catalog property.
    pub const fn default(&self) -> bool {
        self.default
    }
    /// Retained `key_type` catalog property.
    pub const fn key_type(&self) -> &Option<CatalogTypeIdentity> {
        &self.key_type
    }
    /// Exact schema-qualified catalog name, without display-string parsing.
    pub fn qualified_name(&self) -> FullName {
        FullName::with_schema(&self.schema, &self.name)
    }
}

#[cfg(test)]
mod tests;

pub(crate) fn lookup_collation(
    context: &impl BaseParseContext,
    name: &FullName,
) -> CatalogNameLookup {
    if Dialect::from(context.metadata()) == Dialect::sqlite() {
        return context.lookup_sqlite_collation(name);
    }
    lookup_postgres(context, name, |key| {
        context.collation_definition(key).is_some()
    })
}

pub(crate) fn lookup_operator_class(
    context: &impl BaseParseContext,
    name: &FullName,
    method: &str,
) -> CatalogNameLookup {
    if Dialect::from(context.metadata()) == Dialect::sqlite() {
        return CatalogNameLookup::missing(name);
    }
    lookup_postgres(context, name, |key| {
        context.operator_class_definition(key, method).is_some()
    })
}

fn lookup_postgres(
    context: &impl BaseParseContext,
    name: &FullName,
    exists: impl Fn(&FullName) -> bool,
) -> CatalogNameLookup {
    assert!(
        context.database_collation().is_some(),
        "PostgreSQL master definitions must be supplied by the selected base catalog"
    );
    if name.catalog().is_some() {
        return CatalogNameLookup::missing(name);
    }
    if name.schema().is_some() {
        return if exists(name) {
            CatalogNameLookup::found(name, name)
        } else {
            CatalogNameLookup::missing(name)
        };
    }
    let path = context.get_search_path();
    let mut explicit_catalog = false;
    for schema in path {
        if schema.as_str() == "pg_catalog" {
            explicit_catalog = true;
            break;
        }
    }
    if !explicit_catalog {
        let key = FullName::with_schema("pg_catalog", name.name().as_str());
        if exists(&key) {
            return CatalogNameLookup::found(name, &key);
        }
    }
    for schema in path {
        // These master namespaces never search the session's temporary schema.
        if schema.as_str() == "pg_temp" || schema.as_str().starts_with("pg_temp_") {
            continue;
        }
        let key = name.replace_schema(schema);
        if exists(&key) {
            return CatalogNameLookup::found(name, &key);
        }
    }
    CatalogNameLookup::missing(name)
}
