use crate::{
    ast::Dialect,
    context::BaseParseContext,
    names::FullName,
    sem::{CatalogNameLookup, OperatorFamilyKey, TypeReference},
};

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
    array_element: Option<Box<Self>>,
}

impl CatalogTypeIdentity {
    fn from_reference(reference: &TypeReference) -> Self {
        let name = reference
            .full_name()
            .expect("resolved declaration type has a name");
        let schema = name
            .schema()
            .expect("resolved declaration type has a schema")
            .as_str()
            .to_owned();
        let local = name.name().as_str();
        let name = if reference.is_array() == Some(true) {
            format!("_{local}")
        } else {
            local.to_owned()
        };
        Self {
            schema,
            name,
            array_element: None,
        }
    }

    pub(crate) fn type_reference(&self) -> TypeReference {
        if let Some(element) = &self.array_element {
            TypeReference::concrete_type_ref(&element.qualified_name(), true)
        } else if self.schema == "pg_catalog" {
            TypeReference::from_master_type_name(&self.name)
        } else {
            TypeReference::concrete_type_ref(&self.qualified_name(), false)
        }
    }

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
    #[serde(skip)]
    declared_input: Option<TypeReference>,
}

impl OperatorClassDefinition {
    pub(crate) fn from_declaration(
        name: &FullName,
        family: &OperatorFamilyKey,
        input: &TypeReference,
        default: bool,
        storage: Option<&TypeReference>,
    ) -> Self {
        Self {
            schema: name
                .schema()
                .expect("class is qualified")
                .as_str()
                .to_owned(),
            name: name.name().as_str().to_owned(),
            access_method: family.method.clone(),
            family: OperatorFamilyIdentity {
                schema: family
                    .name
                    .schema()
                    .expect("family is qualified")
                    .as_str()
                    .to_owned(),
                name: family.name.name().as_str().to_owned(),
                access_method: family.method.clone(),
            },
            input_type: CatalogTypeIdentity::from_reference(input),
            default,
            key_type: storage.map(CatalogTypeIdentity::from_reference),
            declared_input: Some(input.clone()),
        }
    }

    pub(crate) fn belongs_to(&self, key: &OperatorFamilyKey) -> bool {
        self.family.access_method() == key.method && self.family.qualified_name() == key.name
    }

    pub(crate) fn has_input(&self, typ: &TypeReference) -> bool {
        match &self.declared_input {
            Some(input) => input == typ,
            None => self.input_type.type_reference() == *typ,
        }
    }

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
mod test_extracted_class_input {
    use crate::{
        names::FullName,
        sem::{OperatorClassDefinition, TypeReference},
    };

    #[test]
    fn uses_catalog_array_element_even_when_the_name_has_no_underscore() -> testresult::TestResult {
        let class: OperatorClassDefinition = serde_json::from_value(serde_json::json!({
            "schema": "public", "name": "widgets", "access_method": "btree",
            "family": {"schema": "public", "name": "widgets", "access_method": "btree"},
            "input_type": {
                "schema": "public", "name": "array_catalog_name",
                "array_element": {"schema": "public", "name": "widget"}
            },
            "default": true, "key_type": null
        }))?;
        assert!(class.has_input(&TypeReference::concrete_type_ref(
            &FullName::with_schema("public", "widget"),
            true,
        )));
        assert!(!class.has_input(&TypeReference::concrete_type_ref(
            &FullName::with_schema("public", "array_catalog_name"),
            false,
        )));
        Ok(())
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

#[cfg(test)]
mod test_lookup_collation {

    use crate::{
        names::{FullName, SchemaName},
        sem::{ParseContext, conflict_masters::lookup_collation},
        test_helpers::conflict_masters as test_helpers,
    };

    #[rstest::rstest]
    #[case::unqualified("C", None, Some("pg_catalog"))]
    #[case::qualified("C", Some("pg_catalog"), Some("pg_catalog"))]
    #[case::missing("absent", None, None)]
    #[case::wrong_schema("C", Some("absent"), None)]
    fn resolves_collation_names(
        #[case] name: &str,
        #[case] schema: Option<&str>,
        #[case] expected: Option<&str>,
    ) {
        let fixture = test_helpers::Masters::new();
        let context = fixture.context();
        let requested = if let Some(schema) = schema {
            FullName::with_schema(schema, name)
        } else {
            FullName::from_local_name(name)
        };
        let result = lookup_collation(&context, &requested);
        assert_eq!(result.requested(), &requested);
        assert_eq!(
            result.resolved_name(),
            expected
                .map(|schema| FullName::with_schema(schema, name))
                .as_ref()
        );
        assert_eq!(result.is_missing(), expected.is_none());
    }

    #[test]
    fn ignores_ineligible_schema_before_shadowing() {
        let mut fixture = test_helpers::Masters::new();
        fixture.add_collation("first", "choice", 8, "c");
        fixture.add_collation("second", "choice", -1, "c");
        let mut context = fixture.context();
        context.set_search_path(&[SchemaName::from("first"), SchemaName::from("second")]);
        let result = lookup_collation(&context, &FullName::from_local_name("choice"));
        assert_eq!(
            result.resolved_name(),
            Some(&FullName::with_schema("second", "choice"))
        );
    }

    #[rstest::rstest]
    #[case::implicit_catalog(false, "pg_catalog")]
    #[case::explicit_catalog(true, "public")]
    fn honors_catalog_position(#[case] explicit_catalog: bool, #[case] expected: &str) {
        let mut fixture = test_helpers::Masters::new();
        fixture.add_collation("public", "C", -1, "c");
        let mut context = fixture.context();
        let mut path = vec![SchemaName::from("public")];
        if explicit_catalog {
            path.push(SchemaName::from("pg_catalog"));
        }
        context.set_search_path(&path);
        let result = lookup_collation(&context, &FullName::from_local_name("C"));
        assert_eq!(
            result.resolved_name(),
            Some(&FullName::with_schema(expected, "C"))
        );
    }

    #[test]
    fn unqualified_lookup_skips_temporary_namespaces() {
        let mut fixture = test_helpers::Masters::new();
        fixture.add_collation("pg_temp_42", "temp_only", -1, "c");
        let mut context = fixture.context();
        context.set_search_path(&[SchemaName::from("pg_temp_42")]);
        assert!(lookup_collation(&context, &FullName::from_local_name("temp_only")).is_missing());
        assert!(
            lookup_collation(&context, &FullName::with_schema("pg_temp_42", "temp_only"))
                .resolved_name()
                .is_some()
        );
    }
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

#[cfg(test)]
mod test_lookup_operator_class {

    use crate::{
        context::ConflictInfoRead, names::FullName, sem::conflict_masters::lookup_operator_class,
        test_helpers::conflict_masters as test_helpers,
    };

    #[test]
    fn operator_class_reads_retain_method_family_type_and_default() {
        let fixture = test_helpers::Masters::new();
        let context = fixture.context();
        let name = FullName::with_schema("public", "ops");
        for method in ["btree", "hash"] {
            let result = lookup_operator_class(&context, &FullName::from_local_name("ops"), method);
            assert_eq!(result.resolved_name(), Some(&name));
            let definition = context.operator_class_definition(&name, method).unwrap();
            assert_eq!(definition.family().access_method(), method);
            assert_eq!(
                definition.input_type().qualified_name(),
                FullName::with_schema("outside", "a_type")
            );
            assert!(definition.default());
            assert_eq!(definition.key_type(), &None);
        }
        assert!(lookup_operator_class(&context, &name, "gin").is_missing());
    }
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
