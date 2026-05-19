mod error;
mod params;

use std::{
    cell::{Ref, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    ffi::OsStr,
    fmt::Display,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use crate::PlaceholderedSql;
use petgraph::graph::NodeIndex;
use walkdir::WalkDir;

pub use self::error::{CteCatalogCheckError, CteCatalogCheckErrorCollection, CteCatalogError};
pub use self::params::{
    CteParameter, CteParameterBinding, CteParameterBindingCollection, CteParameterCollection,
    CteParameterRename, ParameterRenameCollection,
};

/// CTE definition name
#[derive(Debug, serde::Deserialize, Clone, Eq, Hash, PartialEq)]
#[serde(transparent)]
pub struct CteName(String);

impl Display for CteName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// CTE alias name
#[derive(Debug, serde::Deserialize, Clone)]
#[serde(transparent)]
pub struct CteAlias(String);

impl Display for CteAlias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// reference CTE in catalog with alias
#[derive(Debug, serde::Deserialize, Clone)]
pub struct CteCatalogReference {
    cte_name: CteName,
    alias: CteAlias,
}

impl CteCatalogReference {
    /// get a CTE name
    #[must_use]
    pub fn cte_name(&self) -> &CteName {
        &self.cte_name
    }

    /// alias
    #[must_use]
    pub fn alias(&self) -> &CteAlias {
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

    fn set_node_id(&mut self, node_id: NodeIndex) {
        self.node_id = Some(node_id);
    }

    #[track_caller]
    fn node_id(&self) -> NodeIndex {
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
    pub fn has_error(&self) -> bool {
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
    pub fn get_parameters(&self) -> Option<&CteParameterCollection> {
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
        let Some(version) = obj.get(Self::VERSION_KEY).and_then(|v| v.as_str()) else {
            return false;
        };
        let Some(dialect) = value.get(Self::DIALECT_KEY).and_then(|v| v.as_str()) else {
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

        assert_eq!(CteCatalog::is_cte_catalog(&parsed), false);
        Ok(())
    }

    #[test]
    fn test_sql_dialect_missing() -> TestResult {
        const TOML: &str = r#"
sql_fun_cte_catalog_format_version="1"
        "#;
        let parsed: JsonValue = toml::from_str(TOML)?;
        assert_eq!(CteCatalog::is_cte_catalog(&parsed), false);
        Ok(())
    }

    #[test]
    fn invalid_format_version_value() -> TestResult {
        const TOML: &str = r#"
sql_fun_cte_catalog_format_version="2"
sql_dialect="PostgreSQL"
        "#;

        let parsed: JsonValue = toml::from_str(TOML)?;
        assert_eq!(CteCatalog::is_cte_catalog(&parsed), false);
        Ok(())
    }

    #[rstest::rstest]
    #[case("PostgreSQL", true)]
    #[case("SQLite", true)]
    #[case("Unknwon", false)]
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
            let value: serde_json::Value = yaml_serde::from_str(&content)?;
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
        let value = CteCatalog::deserialize_file_to_value(&temp.path().to_path_buf())?;
        assert_eq!(value, json!({}));
        Ok(())
    }

    #[test]
    fn deserialize_file_to_value_toml() -> TestResult {
        let temp = NamedTempFile::with_suffix(".toml")?;
        assert_eq!(temp.path().extension(), Some(OsStr::new("toml")));
        let value = CteCatalog::deserialize_file_to_value(&temp.path().to_path_buf())?;
        assert_eq!(value, json!({}));
        Ok(())
    }

    #[test]
    fn deserialize_file_to_value_yaml() -> TestResult {
        let temp = NamedTempFile::with_suffix(".yaml")?;
        assert_eq!(temp.path().extension(), Some(OsStr::new("yaml")));
        let value = CteCatalog::deserialize_file_to_value(&temp.path().to_path_buf())?;
        assert_eq!(value, json!(null));
        Ok(())
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

    /// get cte by name
    #[must_use]
    pub fn get_cte(&self, cte_name: &CteName) -> Option<Ref<'_, CteCatalogEntry<'_>>> {
        self.entries.get(cte_name).map(|e| e.borrow())
    }

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
            let entry = self.entries.get_mut(&cte_name).expect("shoud exist");
            let mut entry = entry.borrow_mut();
            entry.push_cyclic_dependency(&with_item);
        }
    }

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

    /// check all entry references exist
    #[expect(clippy::missing_panics_doc)]
    pub fn check_dependencies_exist(&mut self) -> bool {
        let mut error_found = false;
        let keys: HashSet<CteName> = self.entries.keys().cloned().collect();
        for key in &keys {
            let mut entry = self
                .entries
                .get_mut(key)
                .expect("key existing")
                .borrow_mut();
            for reference in entry.with.clone() {
                if keys.contains(reference.cte_name()) {
                    entry.dependents.push(key.clone());
                } else {
                    error_found = true;
                    entry.push_undefined_entry_reference(reference.cte_name());
                }
            }
        }
        error_found
    }

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

    fn supported_extension(extension: Option<&OsStr>) -> bool {
        let Some(extension) = extension else {
            return false;
        };
        extension == "toml" || extension == "yaml" || extension == "json"
    }

    /// load cte catalog from specified directory
    ///
    /// # Errors
    ///
    /// - [`CteCatalogError`]: when some errors
    ///
    pub fn load(cte_catalog_dir: &Path) -> Result<Self, CteCatalogError> {
        if cte_catalog_dir.exists() {
            CteCatalogError::raise_path_not_exist(cte_catalog_dir)?;
        }
        if !cte_catalog_dir.is_dir() {
            CteCatalogError::raise_path_is_not_directory(cte_catalog_dir)?;
        }

        let catalog = {
            let mut catalog = Self::default();

            let walk_dir = WalkDir::new(cte_catalog_dir).follow_links(true).into_iter();
            let iter = walk_dir.filter_entry(|e| Self::supported_extension(e.path().extension()));
            for file in iter {
                catalog.load_from(file?.path())?;
            }
            catalog
        };

        Ok(catalog)
    }

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
