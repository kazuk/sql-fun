use crate::sem::{AnalysisError, BaseParseContext, CatalogNameLookup, FullName};
use std::{collections::HashSet, fs::File, io::BufReader, path::Path};

/// Read the explicitly loaded master catalog through the common context.
pub trait MasterInfoRead {
    /// Definitions and source database provenance retained by this catalog.
    fn conflict_masters(&self) -> &ConflictMasters;
}

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
    fn matches_name(&self, name: &FullName) -> bool {
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
    fn matches_name(&self, name: &FullName) -> bool {
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

#[derive(Debug, serde::Deserialize)]
struct CollationPayload {
    database: DatabaseCollation,
    collations: Vec<CollationDefinition>,
}

/// Loaded master definitions. Lookup never opens files or database connections.
#[derive(Debug, Clone)]
pub struct ConflictMasters {
    database: Option<DatabaseCollation>,
    collations: Vec<CollationDefinition>,
    operator_classes: Vec<OperatorClassDefinition>,
    sqlite_collations: Vec<String>,
}

impl ConflictMasters {
    pub(crate) const fn sqlite() -> Self {
        Self {
            sqlite_collations: Vec::new(),
            database: None,
            collations: Vec::new(),
            operator_classes: Vec::new(),
        }
    }

    pub(crate) fn load(collations: &Path, classes: &Path) -> Result<Self, AnalysisError> {
        let payload: CollationPayload = Self::read(collations)?;
        let operator_classes = Self::read(classes)?;
        let result = Self {
            database: Some(payload.database),
            collations: payload.collations,
            operator_classes,
            sqlite_collations: Vec::new(),
        };
        result.validate().map_err(|error| {
            AnalysisError::unexpected_input_error(&format!(
                "invalid masters {} / {}: {error}",
                collations.display(),
                classes.display()
            ))
        })?;
        Ok(result)
    }

    fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AnalysisError> {
        let read = || -> Result<T, Box<dyn std::error::Error>> {
            Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
        };
        read().map_err(|error| {
            AnalysisError::unexpected_input_error(&format!("{}: {error}", path.display()))
        })
    }

    fn validate(&self) -> Result<(), &'static str> {
        let database = self
            .database
            .as_ref()
            .ok_or("missing database provenance")?;
        if !(0..=34).contains(&database.encoding) || database.encoding_name.is_empty() {
            return Err("invalid database encoding");
        }
        let mut identities = HashSet::new();
        let mut has_default = false;
        for collation in &self.collations {
            if collation.schema.is_empty()
                || collation.name.is_empty()
                || !(-1..=34).contains(&collation.encoding)
                || !["b", "c", "d", "i"].contains(&collation.provider.as_str())
            {
                return Err("invalid collation identity/encoding");
            }
            if !identities.insert((&collation.schema, &collation.name, collation.encoding)) {
                return Err("duplicate collation identity");
            }
            let identity = &database.default_collation;
            if collation.schema == identity.schema
                && collation.name == identity.name
                && collation.encoding == identity.encoding
            {
                if collation.provider != "d"
                    || !(collation.encoding == -1 || collation.encoding == database.encoding)
                {
                    return Err("invalid default collation provenance");
                }
                has_default = true;
            }
        }
        if !has_default {
            return Err("missing default collation definition");
        }
        let mut classes = HashSet::new();
        for class in &self.operator_classes {
            if class.schema.is_empty()
                || class.name.is_empty()
                || class.access_method.is_empty()
                || class.family.schema.is_empty()
                || class.family.name.is_empty()
                || class.input_type.schema.is_empty()
                || class.input_type.name.is_empty()
                || class
                    .key_type
                    .as_ref()
                    .is_some_and(|key| key.schema.is_empty() || key.name.is_empty())
            {
                return Err("invalid operator-class identity/reference");
            }
            if class.access_method != class.family.access_method {
                return Err("operator-family access-method mismatch");
            }
            if !classes.insert((&class.schema, &class.name, &class.access_method)) {
                return Err("duplicate operator-class identity");
            }
        }
        Ok(())
    }

    /// Enumerate loaded collation definitions without choosing an encoding variant.
    pub fn collations(&self) -> &[CollationDefinition] {
        &self.collations
    }

    /// Enumerate loaded classes, retaining access-method-specific identities.
    pub fn operator_classes(&self) -> &[OperatorClassDefinition] {
        &self.operator_classes
    }

    /// Source database provenance, absent for a SQLite connection namespace.
    pub const fn database(&self) -> Option<&DatabaseCollation> {
        self.database.as_ref()
    }

    /// Read a collation by its full composite identity, without namespace lookup.
    pub fn collation(&self, identity: &CollationIdentity) -> Option<&CollationDefinition> {
        self.collations.iter().find(|value| {
            value.schema == identity.schema
                && value.name == identity.name
                && value.encoding == identity.encoding
        })
    }

    /// Read the encoding-eligible definition at an exact qualified name.
    /// Encoding-specific definitions precede encoding-independent definitions.
    pub fn qualified_collation(&self, name: &FullName) -> Option<&CollationDefinition> {
        let encoding = self.database.as_ref()?.encoding;
        if let Some(specific) = self
            .collations
            .iter()
            .find(|value| value.matches_name(name) && value.encoding == encoding)
        {
            return Some(specific);
        }
        self.collations.iter().find(|value| {
            value.matches_name(name)
                && value.encoding == -1
                // PostgreSQL's stable encoding IDs: ICU cannot use SQL_ASCII,
                // EUC_JIS_2004, MULE_INTERNAL, LATIN10, or WIN874.
                && (value.provider != "i" || ![0, 5, 7, 17, 21].contains(&encoding))
        })
    }

    /// Read an exact operator-class identity without selecting a default class.
    pub fn operator_class(
        &self,
        name: &FullName,
        access_method: &str,
    ) -> Option<&OperatorClassDefinition> {
        self.operator_classes
            .iter()
            .find(|value| value.matches_name(name) && value.access_method == access_method)
    }

    pub(crate) fn declare_sqlite_collation(&mut self, name: &str) -> Result<(), AnalysisError> {
        if self.database.is_some() || name.is_empty() || name.contains('\0') {
            return Err(AnalysisError::unexpected_input_error(
                "a valid collation name and SQLite catalog are required",
            ));
        }
        if !self
            .sqlite_collations
            .iter()
            .any(|value| value.eq_ignore_ascii_case(name))
        {
            self.sqlite_collations.push(name.to_owned());
        }
        Ok(())
    }

    fn sqlite_lookup(&self, name: &FullName) -> CatalogNameLookup {
        if name.schema().is_some() || name.catalog().is_some() {
            return CatalogNameLookup::missing(name);
        }
        // Builtins belong to the active SQLite namespace even when the shared
        // catalog was originally loaded with PostgreSQL analysis metadata.
        for builtin in ["BINARY", "NOCASE", "RTRIM"] {
            if builtin.eq_ignore_ascii_case(name.name().as_str()) {
                return CatalogNameLookup::found(name, &FullName::from_local_name(builtin));
            }
        }
        if let Some(canonical) = self
            .sqlite_collations
            .iter()
            .find(|value| value.eq_ignore_ascii_case(name.name().as_str()))
        {
            CatalogNameLookup::found(name, &FullName::from_local_name(canonical))
        } else {
            CatalogNameLookup::missing(name)
        }
    }
}

#[cfg(test)]
mod tests;

pub(crate) fn lookup_collation(
    context: &impl BaseParseContext,
    name: &FullName,
) -> CatalogNameLookup {
    if crate::ast::Dialect::from(context.metadata()) == crate::ast::Dialect::sqlite() {
        return context.conflict_masters().sqlite_lookup(name);
    }
    lookup_postgres(context, name, |key| {
        context
            .conflict_masters()
            .qualified_collation(key)
            .is_some()
    })
}

pub(crate) fn lookup_operator_class(
    context: &impl BaseParseContext,
    name: &FullName,
    method: &str,
) -> CatalogNameLookup {
    if crate::ast::Dialect::from(context.metadata()) == crate::ast::Dialect::sqlite() {
        return CatalogNameLookup::missing(name);
    }
    lookup_postgres(context, name, |key| {
        context
            .conflict_masters()
            .operator_class(key, method)
            .is_some()
    })
}

fn lookup_postgres(
    context: &impl BaseParseContext,
    name: &FullName,
    exists: impl Fn(&FullName) -> bool,
) -> CatalogNameLookup {
    assert!(
        context.conflict_masters().database().is_some(),
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
