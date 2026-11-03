mod error;
mod params;

pub use self::{
    error::{
        CteCatalogCheckError, CteCatalogCheckErrorCollection, CteCatalogError,
        CteCatalogValidationError,
    },
    params::{
        CteParameter, CteParameterBinding, CteParameterBindingCollection, CteParameterCollection,
        CteParameterRename, ParameterRenameCollection,
    },
};

use crate::{Hasher, ParameterName as LocalParameterName};

use std::{
    cell::{Ref, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    ffi::OsStr,
    fmt::Display,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use crate::{PlaceholderedSql, SqlDialect};
use ::indexmap::IndexMap;
use ::petgraph::graph::NodeIndex;
use ::walkdir::WalkDir;

/// CTE definition name
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Eq, Hash, PartialEq)]
#[serde(transparent)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct CteName(String);

impl PartialEq<str> for CteName {
    fn eq(&self, other: &str) -> bool {
        self.0.eq(other)
    }
}

impl CteName {
    /// create instance
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }
}

impl Display for CteName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl<S> PartialEq<S> for CteName
where
    S: AsRef<str>,
{
    fn eq(&self, other: &S) -> bool {
        self.0 == other.as_ref()
    }
}

/// CTE alias name
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Eq, Hash, PartialEq)]
#[serde(transparent)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct CteAlias(String);

impl PartialEq<str> for CteAlias {
    fn eq(&self, other: &str) -> bool {
        self.0.eq(other)
    }
}

impl Display for CteAlias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl CteAlias {
    /// Resolved alias spelling used in the application query.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// create instance
    #[must_use]
    pub fn new(alias: &str) -> Self {
        Self(String::from(alias))
    }
}

/// reference CTE in catalog with alias
#[derive(Debug, serde::Deserialize, Clone)]
pub struct CteCatalogReference {
    cte_name: CteName,
    alias: CteAlias,
}

impl CteCatalogReference {
    /// create instance
    #[must_use]
    pub fn new(cte_name: &CteName, alias: &CteAlias) -> Self {
        Self {
            cte_name: cte_name.clone(),
            alias: alias.clone(),
        }
    }

    /// get a CTE name
    #[must_use]
    pub const fn cte_name(&self) -> &CteName {
        &self.cte_name
    }

    /// alias
    #[must_use]
    pub const fn alias(&self) -> &CteAlias {
        &self.alias
    }
}

/// CTE definition in CTE catalog
#[derive(Debug)]
pub struct CteCatalogEntry<'a> {
    cte_name: CteName,
    query: PlaceholderedSql,
    with: Vec<CteCatalogReference>,
    renames: ParameterRenameCollection,
    source_file: CatalogFileIndex<'a>,
    errors: CteCatalogCheckErrorCollection<'a>,
    dependents: Vec<CteName>,
    has_implecit_caller: bool,
    aggregated_cte_parameter: Option<CteParameterCollection>,
    node_id: Option<NodeIndex>,
}

impl<'a> CteCatalogEntry<'a> {
    fn from_raw_entry(
        cte_name: &CteName,
        entry: RawCteCatalogEntry,
        index: CatalogFileIndex<'a>,
        has_implecit_caller: bool,
    ) -> Self {
        let query = PlaceholderedSql::from_query(&entry.query);

        let mut errors = CteCatalogCheckErrorCollection::default();
        if !query.fragments_is_empty() {
            errors.push_fragment_embedding_not_supported();
        }

        Self {
            cte_name: cte_name.clone(),
            query,
            with: entry.with,
            renames: ParameterRenameCollection::from(entry.renames),
            source_file: index,
            errors,
            dependents: Vec::default(),
            has_implecit_caller,
            aggregated_cte_parameter: None,
            node_id: None,
        }
    }

    fn push_duplicate_definition_error(&mut self, file_index: CatalogFileIndex<'a>) {
        self.errors.push_duplicate_definition_error(file_index);
    }

    fn push_cyclic_dependency(&mut self, with_item: &CteCatalogReference) {
        self.errors
            .push_cyclic_dependency(&self.cte_name, with_item);
    }

    fn push_undefined_entry_reference(&mut self, cte_name: &CteName) {
        self.errors.push_undefined_entry_reference(cte_name);
    }

    fn push_unused(&mut self) {
        self.errors.push_unused_cte_entry();
    }

    fn push_errors(&mut self, check_errors: CteCatalogCheckErrorCollection<'a>) {
        self.errors.merge(check_errors);
    }

    const fn set_node_id(&mut self, node_id: NodeIndex) {
        self.node_id = Some(node_id);
    }

    #[track_caller]
    const fn node_id(&self) -> NodeIndex {
        self.node_id.expect("must call set_node_id")
    }

    fn list_cyclic_dependency(&self, catalog: &CteCatalog<'_>) -> Vec<CteCatalogReference> {
        let source_id = self.node_id();
        let graph = &catalog.dependencies;
        let mut results = Vec::new();

        for dep in &self.with {
            let Some(dep_entry) = catalog.get_cte(dep.cte_name()) else {
                continue;
            };
            let target_id = dep_entry.node_id();
            if petgraph::algo::has_path_connecting(graph, target_id, source_id, None) {
                results.push(dep.clone());
            }
        }
        results
    }

    /// True when have error
    #[must_use]
    pub const fn has_error(&self) -> bool {
        !self.errors.is_empty()
    }

    /// ready for parameter fixup
    #[must_use]
    pub fn ready_for_parameter_fixup(&self, catalog: &CteCatalog<'a>) -> bool {
        for dep in &self.with {
            if let Some(dep_entry) = catalog.get_cte(dep.cte_name())
                && dep_entry.aggregated_cte_parameter.is_none()
            {
                return false;
            }
        }
        true
    }

    /// fixup parameters
    ///
    /// # Errors
    ///
    /// [`CteCatalogError`]
    ///
    pub fn fixup_parameters(
        &mut self,
        catalog: &CteCatalog<'a>,
    ) -> Result<CteParameterCollection, CteCatalogError> {
        let mut result = CteParameterCollection::default();
        for (name, type_hints) in self.query.parameters() {
            result.push_query_parameter(&self.cte_name, name, type_hints);
        }

        let mut errors = CteCatalogCheckErrorCollection::default();
        for (_param_name, rename_defs) in self.renames.iter() {
            for rename_def in rename_defs {
                if let Some(cte) = catalog.get_cte(rename_def.cte_name()) {
                    let Some(params) = cte.get_parameters() else {
                        CteCatalogError::unexpected_parameter_collection_not_exist()?;
                        CteCatalogError::never()
                    };
                    if !params.exists(rename_def.parameter_name()) {
                        errors.push_parameter_not_found(rename_def);
                    }
                } else {
                    errors.push_undefined_entry_reference(rename_def.cte_name());
                }
            }
        }
        self.push_errors(errors);

        for cte in &self.with {
            let Some(cte_def) = catalog.get_cte(cte.cte_name()) else {
                CteCatalogError::raise_cte_not_found(cte.cte_name())?;
                CteCatalogError::never()
            };

            for (parameter_name, binding) in cte_def.try_get_parameters()? {
                if let Some(rename) = self.renames.get_with(cte.cte_name(), parameter_name) {
                    result.push_binding_with_rename(rename, binding);
                } else {
                    result.push_binding(parameter_name, binding);
                }
            }
        }

        Ok(result)
    }

    /// get parameters
    #[must_use]
    pub const fn get_parameters(&self) -> Option<&CteParameterCollection> {
        self.aggregated_cte_parameter.as_ref()
    }

    /// get parameters
    ///
    /// # Errors
    ///
    /// [`CteCatalogError`] when parameter collection not existing
    ///
    pub fn try_get_parameters(&self) -> Result<&CteParameterCollection, CteCatalogError> {
        let Some(parameters) = &self.aggregated_cte_parameter else {
            CteCatalogError::unexpected_parameter_collection_not_exist()?;
            CteCatalogError::never()
        };
        Ok(parameters)
    }

    /// get check errors on entry
    #[must_use]
    pub const fn errors(&self) -> &CteCatalogCheckErrorCollection<'_> {
        &self.errors
    }

    /// get validation errors on entry
    #[must_use]
    pub fn validation_errors(&self, catalog: &CteCatalog<'a>) -> Vec<CteCatalogValidationError> {
        self.errors.validation_errors(catalog, &self.cte_name)
    }

    /// expand CTE with dependencies
    ///
    /// # Errors
    ///
    /// [`CteCatalogError`]
    fn expand(
        &self,
        catalog: &CteCatalog<'a>,
        alias: &CteAlias,
        state: &mut CteResolutionState,
    ) -> Result<(), CteCatalogError> {
        if state.already_expanded(alias, &self.cte_name)? {
            return Ok(());
        }
        for cte in &self.with {
            let Some(entry) = catalog.get_cte(cte.cte_name()) else {
                CteCatalogError::raise_cte_not_found(cte.cte_name())?;
                CteCatalogError::never()
            };
            entry.expand(catalog, cte.alias(), state)?;
        }
        if !state.already_expanded(alias, &self.cte_name)? {
            state.insert(alias, &self.cte_name, &self.query);
        }
        Ok(())
    }
}

/// Definition identity is retained only during one validated catalog expansion.
struct CteResolutionState {
    dialect: SqlDialect,
    definitions: IndexMap<CteAlias, CteName>,
    queries: IndexMap<CteAlias, PlaceholderedSql>,
}

impl CteResolutionState {
    fn new(dialect: SqlDialect) -> Self {
        Self {
            dialect,
            definitions: IndexMap::new(),
            queries: IndexMap::new(),
        }
    }

    fn already_expanded(&self, alias: &CteAlias, name: &CteName) -> Result<bool, CteCatalogError> {
        for (existing_alias, existing_name) in &self.definitions {
            if self
                .dialect
                .same_identifier(existing_alias.as_str(), alias.as_str())
            {
                if existing_name == name {
                    return Ok(true);
                }
                return CteCatalogError::conflicting_alias(
                    existing_alias,
                    existing_name,
                    alias,
                    name,
                );
            }
        }
        Ok(false)
    }

    fn insert(&mut self, alias: &CteAlias, name: &CteName, query: &PlaceholderedSql) {
        self.definitions.insert(alias.clone(), name.clone());
        self.queries.insert(alias.clone(), query.clone());
    }
}

#[derive(Debug, serde::Deserialize)]
struct RawCteCatalogEntry {
    query: String,
    #[serde(default)]
    with: Vec<CteCatalogReference>,
    #[serde(default)]
    renames: HashMap<String, Vec<CteParameterRename>>,
}

#[derive(Debug, serde::Deserialize)]
struct RawCatalogFile {
    cte: HashMap<CteName, RawCteCatalogEntry>,
}

/// Associate usize index to `PathBuff`
#[derive(Debug, Clone, Copy)]
pub struct CatalogFileIndex<'a> {
    index: usize,
    _phantom: PhantomData<&'a [PathBuf]>,
}

type CteDependencyGraph = ::petgraph::graph::DiGraph<CteName, ()>;

/// CTE catalog
#[derive(Debug, Default)]
pub struct CteCatalog<'a> {
    entries: HashMap<CteName, RefCell<CteCatalogEntry<'a>>>,
    files: Vec<PathBuf>,
    dependencies: CteDependencyGraph,
}

impl CteCatalog<'_> {
    const VERSION_KEY: &'static str = "sql_fun_cte_catalog_format_version";
    const DIALECT_KEY: &'static str = "sql_dialect";
    const SUPPORTED_DIALECT: &'static [&'static str] = &["PostgreSQL", "SQLite"];

    fn is_cte_catalog(value: &serde_json::Value) -> bool {
        let Some(obj) = value.as_object() else {
            return false;
        };
        let Some(version) = obj.get(Self::VERSION_KEY) else {
            return false;
        };
        let Some(version) = version.as_str() else {
            return false;
        };
        let Some(dialect) = value.get(Self::DIALECT_KEY) else {
            return false;
        };
        let Some(dialect) = dialect.as_str() else {
            return false;
        };
        version == "1" && Self::SUPPORTED_DIALECT.contains(&dialect)
    }
}

#[cfg(test)]
mod cte_catalog_is_cte_catalog_tests {
    use crate::CteCatalog;
    use serde_json::Value as JsonValue;
    use testresult::TestResult;

    #[test]
    fn test_cte_catalog_format_version_missing() -> TestResult {
        const TOML: &str = r#"
sql_dialect = "PostgreSQL"
        "#;
        let parsed: JsonValue = toml::from_str(TOML)?;

        assert!(!CteCatalog::is_cte_catalog(&parsed));
        Ok(())
    }

    #[test]
    fn test_sql_dialect_missing() -> TestResult {
        const TOML: &str = r#"
sql_fun_cte_catalog_format_version="1"
        "#;
        let parsed: JsonValue = toml::from_str(TOML)?;
        assert!(!CteCatalog::is_cte_catalog(&parsed));
        Ok(())
    }

    #[test]
    fn invalid_format_version_value() -> TestResult {
        const TOML: &str = r#"
sql_fun_cte_catalog_format_version="2"
sql_dialect="PostgreSQL"
        "#;

        let parsed: JsonValue = toml::from_str(TOML)?;
        assert!(!CteCatalog::is_cte_catalog(&parsed));
        Ok(())
    }

    #[rstest::rstest]
    #[case("PostgreSQL", true)]
    #[case("SQLite", true)]
    #[case("Unknown", false)]
    fn dialect_values(#[case] dialect: &str, #[case] expected: bool) -> TestResult {
        let toml = format!(
            r#"
sql_fun_cte_catalog_format_version="1"
sql_dialect="{dialect}"
"#
        );
        let parsed: JsonValue = toml::from_str(&toml)?;
        assert_eq!(CteCatalog::is_cte_catalog(&parsed), expected);
        Ok(())
    }
}

impl CteCatalog<'_> {
    fn into_entries(
        root: serde_json::Value,
    ) -> Result<HashMap<CteName, RawCteCatalogEntry>, CteCatalogError> {
        let raw_file: RawCatalogFile = serde_json::from_value(root)?;
        Ok(raw_file.cte)
    }
}

#[cfg(test)]
mod cte_catalog_into_entries_tests {
    use crate::CteCatalog;
    use serde_json::Value as JsonValue;
    use testresult::TestResult;

    #[test]
    fn test_into_entries_simple() -> TestResult {
        const TOML: &str = r#"
[cte.active_users]
query="""
select u.*
from users u
where u.deleted_at is NULL
"""

[cte.user_orderes]
query="""
select o.*
from
  orders o
inner join
  active_users au
  on o.user_id = au.id
where o.user_id = ${user_id}
"""
with=[
    { cte_name="active_users", alias="active_users" }
]

[cte.order_items]
query="""
select oi.*
from
  user_orders o
inner join
  order_items oi
    on o.id = oi.order_id
inner join
  active_users au
    on o.user_id = au.id
where o.user_id = ${user_id}
  and o.id = ${order_id}
"""
with=[
    { cte_name="active_users", alias="active_users" },
    { cte_name="user_orders", alias="user_orders" }
]

"#;
        let parsed: JsonValue = toml::from_str(TOML)?;
        let _entries = CteCatalog::into_entries(parsed)?;
        Ok(())
    }
}

impl CteCatalog<'_> {
    fn deserialize_file_to_value(file_path: &Path) -> Result<serde_json::Value, CteCatalogError> {
        let Some(extension) = file_path.extension() else {
            unreachable!()
        };
        let content = std::fs::read_to_string(file_path)?;
        let value = if extension == "json" {
            let value: serde_json::Value = serde_json::from_str(&content)?;
            value
        } else if extension == "toml" {
            let value: serde_json::Value = toml::from_str(&content)?;
            value
        } else if extension == "yaml" {
            let value: serde_json::Value = serde_saphyr::from_str(&content)?;
            value
        } else {
            unreachable!();
        };
        Ok(value)
    }
}

#[cfg(test)]
mod cte_catalog_deserialize_file_to_value_tests {
    use crate::CteCatalog;
    use serde_json::json;
    use std::{ffi::OsStr, io::Write};
    use tempfile::NamedTempFile;
    use testresult::TestResult;

    #[test]
    fn deserialize_file_to_value_json() -> TestResult {
        let mut temp = NamedTempFile::with_suffix(".json")?;
        assert_eq!(temp.path().extension(), Some(OsStr::new("json")));
        temp.as_file_mut().write_all("{}".as_bytes())?;
        let value = CteCatalog::deserialize_file_to_value(temp.path())?;
        assert_eq!(value, json!({}));
        Ok(())
    }

    #[test]
    fn deserialize_file_to_value_toml() -> TestResult {
        let temp = NamedTempFile::with_suffix(".toml")?;
        assert_eq!(temp.path().extension(), Some(OsStr::new("toml")));
        let value = CteCatalog::deserialize_file_to_value(temp.path())?;
        assert_eq!(value, json!({}));
        Ok(())
    }

    #[test]
    fn deserialize_file_to_value_yaml() -> TestResult {
        let temp = NamedTempFile::with_suffix(".yaml")?;
        assert_eq!(temp.path().extension(), Some(OsStr::new("yaml")));
        let value = CteCatalog::deserialize_file_to_value(temp.path())?;
        assert_eq!(value, json!(null));
        Ok(())
    }

    #[test]
    fn yaml_preserves_nested_catalog_aliases_and_multiline_queries() -> TestResult {
        let file = test_helpers::catalog_yaml(false)?;
        let value = CteCatalog::deserialize_file_to_value(file.path())?;
        assert_eq!(value["cte"]["users"]["query"], "SELECT id\nFROM users\n");
        assert_eq!(value["cte"]["users"], value["cte"]["alias"]);
        Ok(())
    }

    #[test]
    fn malformed_yaml_is_rejected() -> TestResult {
        let file = test_helpers::catalog_yaml(true)?;
        assert!(CteCatalog::deserialize_file_to_value(file.path()).is_err());
        Ok(())
    }

    mod test_helpers {
        pub(super) fn catalog_yaml(
            malformed: bool,
        ) -> testresult::TestResult<::tempfile::NamedTempFile> {
            let file = ::tempfile::NamedTempFile::with_suffix(".yaml")?;
            let text = if malformed {
                "cte: [unfinished"
            } else {
                r"cte:
  users: &users
    query: |
      SELECT id
      FROM users
  alias: *users
"
            };
            std::fs::write(file.path(), text)?;
            Ok(file)
        }
    }
}

impl CteCatalog<'_> {
    /// load CTE catalog file from specified path
    ///
    /// # Errors
    ///
    /// [`CteCatalogError`] : when error occurs
    pub fn load_from(&mut self, file_path: &Path) -> Result<(), CteCatalogError> {
        if !Self::supported_extension(file_path.extension()) {
            CteCatalogError::raise_unsupported_file_format(file_path)?;
        }
        let value = Self::deserialize_file_to_value(file_path)?;
        if !Self::is_cte_catalog(&value) {
            CteCatalogError::raise_file_is_not_cte_catalog(file_path)?;
        }
        let raw_entries = Self::into_entries(value)?;
        let file_index = self.push_file(file_path);
        for (cte_name, raw_entry) in raw_entries {
            let entry = CteCatalogEntry::from_raw_entry(&cte_name, raw_entry, file_index, false);
            self.entries
                .entry(entry.cte_name.clone())
                .and_modify(|o| o.borrow_mut().push_duplicate_definition_error(file_index))
                .or_insert(RefCell::new(entry));
        }
        Ok(())
    }
}

#[cfg(test)]
mod cte_catalog_load_from_tests {
    use assert_matches::assert_matches;
    use testresult::TestResult;

    use crate::test_helpers::{TestProjectDir, write_cte_catalog_toml};

    use crate::cte_catalog::{CteCatalog, CteCatalogError, CteName};

    #[test]
    fn load_from_registers_entry_and_source_file() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let catalog_file = write_cte_catalog_toml(
            &project_dir,
            "cte/users.toml",
            r#"[cte.active_users]
query = "select * from users where id = ${user_id}"
"#,
        )?;
        let mut catalog = CteCatalog::default();

        catalog.load_from(&catalog_file)?;

        let active_users = CteName::new("active_users");
        let entry = catalog.get_cte(&active_users).expect("cte should exist");
        assert_eq!(catalog.get_file_of(&entry), &catalog_file);
        assert!(!entry.has_error());
        Ok(())
    }

    #[test]
    fn load_from_rejects_non_catalog_file() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let catalog_file = project_dir.write_file(
            "cte/not_catalog.toml",
            "[cte.active_users]
query = \"select * from users\"
",
        )?;
        let mut catalog = CteCatalog::default();

        let err = catalog
            .load_from(&catalog_file)
            .expect_err("should reject missing header");
        assert_matches!(
            err,
            CteCatalogError::FileIsNotCteCatalog(path) if path == catalog_file
        );
        Ok(())
    }
}

impl<'a> CteCatalog<'a> {
    /// get cte by name
    #[must_use]
    pub fn get_cte(&self, cte_name: &CteName) -> Option<Ref<'_, CteCatalogEntry<'a>>> {
        self.entries.get(cte_name).map(|e| e.borrow())
    }
}

impl CteCatalog<'_> {
    fn get_node_id(&self, cte_name: &CteName) -> Option<NodeIndex> {
        self.entries.get(cte_name).map(|e| e.borrow().node_id())
    }

    /// build dependency graph
    #[expect(clippy::missing_panics_doc)]
    pub fn build_dependency_graph(&mut self) {
        // register nodes
        for (cte_name, entry) in &mut self.entries {
            let mut entry = entry.borrow_mut();
            let node_id = self.dependencies.add_node(cte_name.clone());
            entry.set_node_id(node_id);
        }

        // register edges
        for entry in self.entries.values() {
            let entry = entry.borrow_mut();
            let source_id = entry.node_id();

            for dep in &entry.with {
                if let Some(target_id) = self.get_node_id(dep.cte_name()) {
                    self.dependencies.add_edge(source_id, target_id, ());
                }
            }
        }

        let cyclic_dependencies = self.list_cyclic_dependencies();
        for (cte_name, with_item) in cyclic_dependencies {
            let entry = self.entries.get_mut(&cte_name).expect("should exist");
            let mut entry = entry.borrow_mut();
            entry.push_cyclic_dependency(&with_item);
        }
    }
}

#[cfg(test)]
mod cte_catalog_build_dependency_graph_tests {
    use testresult::TestResult;

    use crate::{
        CteCatalog, CteCatalogCheckError, CteName,
        test_helpers::{TestProjectDir, cte_catalog_has_error, write_cte_catalog_toml},
    };

    #[test]
    fn build_dependency_graph_marks_cycles() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let catalog_file = write_cte_catalog_toml(
            &project_dir,
            "cte/catalog.toml",
            r#"[cte.a]
query = "select * from b"
with = [{ cte_name = "b", alias = "b" }]

[cte.b]
query = "select * from a"
with = [{ cte_name = "a", alias = "a" }]
"#,
        )?;
        let mut catalog = CteCatalog::default();
        catalog.load_from(&catalog_file)?;

        assert!(!catalog.check_dependencies_exist());
        catalog.build_dependency_graph();

        assert_eq!(catalog.dependencies.node_count(), 2);
        assert_eq!(catalog.dependencies.edge_count(), 2);
        assert!(cte_catalog_has_error(&catalog, "a", |error| matches!(
            error,
            CteCatalogCheckError::CyclicDependency(name, _) if name == &CteName::new("a")
        )));
        assert!(cte_catalog_has_error(&catalog, "b", |error| matches!(
            error,
            CteCatalogCheckError::CyclicDependency(name, _) if name == &CteName::new("b")
        )));
        Ok(())
    }
}

impl CteCatalog<'_> {
    fn list_cyclic_dependencies(&self) -> Vec<(CteName, CteCatalogReference)> {
        let mut results = Vec::new();
        for (cte_name, entry) in &self.entries {
            let entry = entry.borrow();
            for dep in entry.list_cyclic_dependency(self) {
                results.push((cte_name.clone(), dep));
            }
        }
        results
    }

    /// fixup CTE parameters
    ///
    #[expect(clippy::missing_panics_doc)]
    pub fn fixup_parameters(&mut self) {
        let mut cte_names: VecDeque<CteName> = self.entries.keys().cloned().collect();

        while let Some(cte_name) = cte_names.pop_front() {
            let agg_params = {
                let cte = self.entries.get(&cte_name).expect("exists");
                let mut cte = cte.borrow_mut();
                if cte.has_error() {
                    continue;
                }
                if !cte.ready_for_parameter_fixup(self) {
                    cte_names.push_back(cte_name);
                    continue;
                }
                cte.fixup_parameters(self)
            };
            if let Ok(agg_params) = agg_params {
                let cte = self.entries.get_mut(&cte_name).expect("exists");
                let mut cte = cte.borrow_mut();
                cte.aggregated_cte_parameter = Some(agg_params);
            }
        }
    }
}

#[cfg(test)]
mod cte_ctalog_fixup_parameters_tests {
    use testresult::TestResult;

    use crate::{
        CteCatalog, CteName,
        placehlodered_sql::ParameterName,
        test_helpers::{TestProjectDir, write_cte_catalog_toml},
    };

    #[test]
    fn fixup_parameters_cascades_dependency_parameters() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let catalog_file = write_cte_catalog_toml(
            &project_dir,
            "cte/catalog.toml",
            r#"[cte.parent]
query = "select * from child where id = ${parent_id}"
with = [{ cte_name = "child", alias = "child" }]

[cte.child]
query = "select * from users where id = ${child_id}"
"#,
        )?;
        let mut catalog = CteCatalog::default();
        catalog.load_from(&catalog_file)?;

        assert!(!catalog.check_dependencies_exist());
        catalog.fixup_parameters();

        let child = catalog
            .get_cte(&CteName::new("child"))
            .expect("child should exist");
        let child_params = child
            .get_parameters()
            .expect("child parameters should exist");
        assert!(child_params.exists(&ParameterName::from("child_id")));
        assert!(!child_params.exists(&ParameterName::from("parent_id")));

        let parent = catalog
            .get_cte(&CteName::new("parent"))
            .expect("parent should exist");
        let parent_params = parent
            .get_parameters()
            .expect("parent parameters should exist");
        assert!(parent_params.exists(&ParameterName::from("child_id")));
        assert!(parent_params.exists(&ParameterName::from("parent_id")));
        Ok(())
    }
}

impl CteCatalog<'_> {
    /// check all entry references exist
    #[expect(clippy::missing_panics_doc)]
    pub fn check_dependencies_exist(&mut self) -> bool {
        let mut error_found = false;
        let keys: HashSet<CteName> = self.entries.keys().cloned().collect();
        for key in &keys {
            let references = {
                let entry = self.entries.get(key).expect("key existing").borrow();
                entry.with.clone()
            };
            for reference in references {
                if keys.contains(reference.cte_name()) {
                    let mut referenced_entry = self
                        .entries
                        .get_mut(reference.cte_name())
                        .expect("key existing")
                        .borrow_mut();
                    referenced_entry.dependents.push(key.clone());
                } else {
                    error_found = true;
                    let mut entry = self
                        .entries
                        .get_mut(key)
                        .expect("key existing")
                        .borrow_mut();
                    entry.push_undefined_entry_reference(reference.cte_name());
                }
            }
        }
        error_found
    }
}

#[cfg(test)]
mod cte_catalog_check_dependencies_exist_tests {
    use testresult::TestResult;

    use crate::{
        cte_catalog::{CteCatalog, CteCatalogCheckError, CteName},
        test_helpers::{TestProjectDir, cte_catalog_has_error, write_cte_catalog_toml},
    };

    #[test]
    fn check_dependencies_exist_marks_missing_references() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let catalog_file = write_cte_catalog_toml(
            &project_dir,
            "cte/catalog.toml",
            r#"[cte.parent]
query = "select * from missing_cte"
with = [{ cte_name = "missing_cte", alias = "m" }]
"#,
        )?;
        let mut catalog = CteCatalog::default();
        catalog.load_from(&catalog_file)?;

        assert!(catalog.check_dependencies_exist());
        assert!(cte_catalog_has_error(&catalog, "parent", |error| matches!(
            error,
            CteCatalogCheckError::UndefinedEntryReference(name) if name == &CteName::new("missing_cte")
        )));
        Ok(())
    }
}

impl CteCatalog<'_> {
    /// check unused
    pub fn check_unused_definition(&mut self) -> bool {
        let mut error_found = false;
        for entry in self.entries.values_mut() {
            let mut entry = entry.borrow_mut();
            if !entry.has_implecit_caller && entry.dependents.is_empty() {
                error_found = true;
                entry.push_unused();
            }
        }
        error_found
    }
}

#[cfg(test)]
mod cte_catalog_check_unused_definition_tests {
    use testresult::TestResult;

    use crate::{
        CteCatalog, CteCatalogCheckError, CteName,
        test_helpers::{TestProjectDir, cte_catalog_has_error, write_cte_catalog_toml},
    };

    #[test]
    fn check_unused_definition_only_flags_entries_without_dependents() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let catalog_file = write_cte_catalog_toml(
            &project_dir,
            "cte/catalog.toml",
            r#"[cte.parent]
query = "select * from child where id = ${parent_id}"
with = [{ cte_name = "child", alias = "child" }]

[cte.child]
query = "select * from users where id = ${child_id}"
"#,
        )?;
        let mut catalog = CteCatalog::default();
        catalog.load_from(&catalog_file)?;

        assert!(!catalog.check_dependencies_exist());
        assert!(catalog.check_unused_definition());

        let child = catalog
            .get_cte(&CteName::new("child"))
            .expect("child should exist");
        assert_eq!(child.dependents, vec![CteName::new("parent")]);
        assert!(!cte_catalog_has_error(&catalog, "child", |error| matches!(
            error,
            CteCatalogCheckError::UnusedCteEntry
        )));
        assert!(cte_catalog_has_error(&catalog, "parent", |error| matches!(
            error,
            CteCatalogCheckError::UnusedCteEntry
        )));
        Ok(())
    }
}

impl CteCatalog<'_> {
    fn supported_extension(extension: Option<&OsStr>) -> bool {
        let Some(extension) = extension else {
            return false;
        };
        extension == "toml" || extension == "yaml" || extension == "json"
    }
}

#[cfg(test)]
mod cte_catalog_supported_extension_tests {
    use std::ffi::OsStr;

    use crate::cte_catalog::CteCatalog;

    #[test]
    fn supported_extension_values() {
        assert!(CteCatalog::supported_extension(Some(OsStr::new("toml"))));
        assert!(CteCatalog::supported_extension(Some(OsStr::new("yaml"))));
        assert!(CteCatalog::supported_extension(Some(OsStr::new("json"))));
        assert!(!CteCatalog::supported_extension(Some(OsStr::new("txt"))));
        assert!(!CteCatalog::supported_extension(None));
    }
}

impl CteCatalog<'_> {
    /// load cte catalog from specified directory
    ///
    /// # Errors
    ///
    /// - [`CteCatalogError`]: when some errors
    ///
    pub fn load(cte_catalog_dir: &Path) -> Result<Self, CteCatalogError> {
        if !cte_catalog_dir.exists() {
            CteCatalogError::raise_path_not_exist(cte_catalog_dir)?;
        }
        if !cte_catalog_dir.is_dir() {
            CteCatalogError::raise_path_is_not_directory(cte_catalog_dir)?;
        }

        let mut catalog = {
            let mut catalog = Self::default();

            let walk_dir = WalkDir::new(cte_catalog_dir).follow_links(true).into_iter();
            let iter = walk_dir.filter_entry(|e| {
                e.path().is_dir() || Self::supported_extension(e.path().extension())
            });
            for file in iter {
                let file = file?;
                if file.file_type().is_file() {
                    catalog.load_from(file.path())?;
                }
            }
            catalog
        };
        catalog.validate()?;
        Ok(catalog)
    }

    /// validate catalog
    ///
    /// # Errors
    ///
    /// - [`CteCatalogError`]: when some errors
    ///
    pub fn validate(&mut self) -> Result<&mut Self, CteCatalogError> {
        self.check_dependencies_exist();
        self.build_dependency_graph();
        self.fixup_parameters();

        let mut errors = Vec::new();
        for entry in self.entries.values() {
            let entry = entry.borrow();
            if !entry.has_error() {
                continue;
            }
            let mut entry_errors = entry.validation_errors(self);
            errors.append(&mut entry_errors);
        }
        if errors.is_empty() {
            Ok(self)
        } else {
            CteCatalogError::validation_error(errors)?;
            CteCatalogError::never()
        }
    }
}

#[cfg(test)]
mod cte_catalog_load_tests {
    use assert_matches::assert_matches;
    use std::path::Path;
    use testresult::TestResult;

    use crate::{
        CteCatalog, CteCatalogError, CteName,
        test_helpers::{TestProjectDir, write_cte_catalog_toml},
    };

    #[test]
    fn load_reads_supported_files_recursively() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let cte_dir = project_dir.create_dir("cte")?;
        let direct_file = write_cte_catalog_toml(
            &project_dir,
            "cte/direct.toml",
            r#"[cte.direct]
query = "select 1"
"#,
        )?;
        let nested_file = write_cte_catalog_toml(
            &project_dir,
            "cte/nested/child.toml",
            r#"[cte.nested]
query = "select 2"
"#,
        )?;

        let catalog = CteCatalog::load(cte_dir.as_path())?;

        let direct = catalog
            .get_cte(&CteName::new("direct"))
            .expect("direct should exist");
        assert_eq!(catalog.get_file_of(&direct), &direct_file);
        let nested = catalog
            .get_cte(&CteName::new("nested"))
            .expect("nested should exist");
        assert_eq!(catalog.get_file_of(&nested), &nested_file);
        Ok(())
    }

    #[test]
    fn load_rejects_file_path() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let file = project_dir.write_file("cte.toml", "")?;

        let err = CteCatalog::load(Path::new(&file)).expect_err("file path should be rejected");
        assert_matches!(err, CteCatalogError::PathIsNotDirectory(path) if path == file);
        Ok(())
    }
}

impl CteCatalog<'_> {
    fn push_file<'a, P: AsRef<Path>>(&mut self, path: P) -> CatalogFileIndex<'a> {
        let index = self.files.len();
        self.files.push(path.as_ref().to_path_buf());
        CatalogFileIndex {
            index,
            _phantom: PhantomData,
        }
    }

    /// get file path
    ///
    /// # Panics
    ///
    /// index not in catalog
    #[must_use]
    pub fn get_file_path(&self, index: CatalogFileIndex<'_>) -> &PathBuf {
        assert!(index.index < self.files.len());
        &self.files[index.index]
    }

    /// get file for entry
    #[must_use]
    pub fn get_file_of(&self, entry: &CteCatalogEntry<'_>) -> &PathBuf {
        self.get_file_path(entry.source_file)
    }
}

impl CteCatalog<'_> {
    /// Resolve references using the explicitly selected SQL dialect.
    /// Shared definitions under equivalent aliases are emitted once, in dependency
    /// order. Different definitions claiming equivalent aliases are rejected.
    /// The catalog must have passed validation before resolution.
    ///
    /// # Errors
    ///
    /// [`CteCatalogError`]
    ///
    pub fn resolve(
        &self,
        references: &[CteCatalogReference],
        dialect: SqlDialect,
    ) -> Result<ResolvedCteCatalog, CteCatalogError> {
        let mut parameters = CteParameterCollection::default();
        let mut state = CteResolutionState::new(dialect);

        for reference in references {
            let cte_name = &reference.cte_name();
            let Some(cte) = self.get_cte(cte_name) else {
                CteCatalogError::raise_cte_not_found(cte_name)?;
                CteCatalogError::never()
            };
            let params = cte.try_get_parameters()?;
            parameters.marge(params.clone());
            cte.expand(self, reference.alias(), &mut state)?;
        }

        Ok(ResolvedCteCatalog::new(&parameters, &state.queries))
    }
}

#[cfg(test)]
mod resolve_tests;

/// resolved CTE catalog
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct ResolvedCteCatalog {
    parameters: CteParameterCollection,
    #[cfg_attr(
        feature = "openrpc-schema",
        schemars(with = "std::collections::BTreeMap<CteAlias, PlaceholderedSql>")
    )]
    cte_queries: IndexMap<CteAlias, PlaceholderedSql>,
}

impl ResolvedCteCatalog {
    /// Construct from queries whose catalog dependencies have already been resolved.
    #[must_use]
    pub fn new(
        parameters: &CteParameterCollection,
        cte_queries: &IndexMap<CteAlias, PlaceholderedSql>,
    ) -> Self {
        Self {
            parameters: parameters.clone(),
            cte_queries: cte_queries.clone(),
        }
    }

    /// Aggregated binding metadata, independent of placeholder numbering.
    #[must_use]
    pub const fn parameters(&self) -> &CteParameterCollection {
        &self.parameters
    }

    /// Append catalog CTE parameter names in resolved collection order.
    /// The caller establishes placement relative to the application query.
    pub fn append_cte_parameter_order(&self, order: &mut Vec<LocalParameterName>) {
        for query in self.cte_queries.values() {
            query.append_parameter_order(order);
        }
    }

    /// Transform resolved CTEs in dependency/insertion order without resolving again.
    ///
    /// # Errors
    ///
    /// Returns the first error from the supplied transformation.
    pub fn try_map_cte_queries<T, E>(
        &self,
        mut transform: impl FnMut(&CteAlias, &PlaceholderedSql) -> Result<T, E>,
    ) -> Result<Vec<T>, E> {
        self.cte_queries
            .iter()
            .map(|(alias, query)| transform(alias, query))
            .collect()
    }

    /// Compute full query identity from explicit application SQL and resolved dependencies.
    ///
    /// # Errors
    ///
    /// [`CteCatalogError`]
    #[tracing::instrument(ret, err, skip(self, base_query))]
    pub fn finger_print(&self, base_query: &PlaceholderedSql) -> Result<String, CteCatalogError> {
        let mut hasher = Hasher::new();
        hasher.append_str(&serde_json::to_string(&self.parameters)?);
        hasher.append_str(&serde_json::to_string(base_query)?);
        hasher.append_str(&serde_json::to_string(&self.cte_queries)?);
        Ok(hasher.finish())
    }
}

#[cfg(test)]
mod test_resolved_cte_catalog {
    use crate::{SqlDialect, cte_catalog::CteCatalog};

    use crate::{
        ParameterName, PlaceholderedSql,
        cte_catalog::{CteAlias, CteName, CteParameterCollection, ResolvedCteCatalog},
    };
    use ::indexmap::IndexMap;

    fn ordered_names(
        base_query: &PlaceholderedSql,
        catalog: &ResolvedCteCatalog,
    ) -> Vec<ParameterName> {
        let mut names = Vec::new();
        catalog.append_cte_parameter_order(&mut names);
        base_query.append_parameter_order(&mut names);
        names
    }

    #[test]
    fn empty_catalog_keeps_application_parameter_metadata_separate() {
        let base_query = PlaceholderedSql::from_query("select ${id:int}, ${id}, ${other}");
        let catalog = ResolvedCteCatalog::default();
        assert_eq!(
            ordered_names(&base_query, &catalog),
            [ParameterName::from("id"), ParameterName::from("other")]
        );
        assert_eq!(
            ::serde_json::to_string(catalog.parameters()).unwrap(),
            r#"{"parameters":{}}"#
        );
        let name = ParameterName::from("id");
        assert!(catalog.parameters().binding(&name).is_none());
        let (_, hints) = base_query
            .parameters()
            .find(|(parameter_name, _)| *parameter_name == &name)
            .unwrap();
        assert_eq!(hints.len(), 1);
    }

    #[test]
    fn prefix_order_retains_catalog_and_base_binding_metadata() {
        let mut parameters = CteParameterCollection::default();
        let mut queries = IndexMap::new();
        let query = PlaceholderedSql::from_query("select ${shared:int}, ${first}");
        for (name, hints) in query.parameters() {
            parameters.push_query_parameter(&CteName::new("definition"), name, hints);
        }
        queries.insert(CteAlias::new("chosen"), query);
        let base_query = PlaceholderedSql::from_query(
            "with existing as (select ${second:text}) select ${shared}, ${base}",
        );
        let catalog = ResolvedCteCatalog::new(&parameters, &queries);
        assert_eq!(
            ordered_names(&base_query, &catalog),
            ["shared", "first", "second", "base"].map(ParameterName::from)
        );
        let name = ParameterName::from("shared");
        let shared = catalog.parameters().binding(&name).unwrap();
        let bindings = shared.catalog_bindings().collect::<Vec<_>>();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].0, &CteName::new("definition"));
        assert_eq!(bindings[0].1.len(), 1);
        assert_eq!(
            ::serde_json::to_value(catalog.parameters()).unwrap()["parameters"]["shared"],
            ::serde_json::json!([["definition", ["int"]]])
        );
        assert!(
            ::serde_json::to_value(&catalog)
                .unwrap()
                .get("base_query")
                .is_none()
        );
        let restored: ResolvedCteCatalog =
            ::serde_json::from_str(&::serde_json::to_string(&catalog).unwrap()).unwrap();
        assert_eq!(
            ordered_names(&base_query, &restored),
            ordered_names(&base_query, &catalog)
        );
    }

    #[test]
    fn resolver_keeps_base_only_metadata_in_query() {
        let base_query = PlaceholderedSql::from_query("select ${base:int}");
        let catalog = CteCatalog::default()
            .resolve(&[], SqlDialect::PostgreSQL)
            .unwrap();
        let name = ParameterName::from("base");
        assert!(catalog.parameters().binding(&name).is_none());
        let (_, hints) = base_query
            .parameters()
            .find(|(parameter_name, _)| *parameter_name == &name)
            .unwrap();
        assert_eq!(hints.len(), 1);
    }
}

#[cfg(test)]
mod fingerprint_tests;
