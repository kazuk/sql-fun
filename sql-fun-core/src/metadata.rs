use crate::{CodegenOptions, LintConfigurationError, LintConfigurations, RewriteOptions};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use either::Either;

use crate::{
    PostgresExtensionsCollection, PostgresVersion, SqlDialect, SqlFunArgs, SqlFunArgsError,
};

/// Engine choices for initialization, independent of enabled analysis features.
/// Full analysis metadata is resolved only for selected implemented setup tasks.
pub struct InitializationDatasources {
    datasources: toml::Table,
    metadata_file: PathBuf,
}

impl InitializationDatasources {
    /// Read datasource declarations without choosing or initializing an engine.
    ///
    /// # Errors
    /// Rejects unreadable or malformed project metadata.
    pub fn load_from(path: &Path) -> Result<Self, MetadataError> {
        let text = std::fs::read_to_string(path)?;
        let document: toml::Value = toml::from_str(&text)?;
        let database = if let Some(package) = document.get("package")
            && let Some(metadata) = package.get("metadata")
        {
            metadata.get("database")
        } else {
            None
        };
        let Some(database) = database else {
            return Ok(Self {
                datasources: toml::Table::new(),
                metadata_file: path.to_path_buf(),
            });
        };
        let Some(table) = database.as_table() else {
            return MetadataError::metadata_load_from_file(
                path,
                "package.metadata.database requires a table",
            );
        };
        Ok(Self {
            datasources: table.clone(),
            metadata_file: path.to_path_buf(),
        })
    }

    /// Datasource names in deterministic prompt order.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names = self
            .datasources
            .iter()
            .filter(|(_, value)| value.is_table())
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    /// The explicitly configured engine of a selected datasource.
    ///
    /// # Errors
    /// Rejects missing datasources or omitted engine choices, even for one option.
    pub fn engine(&self, name: &str) -> Result<&str, MetadataError> {
        if let Some(datasource) = self.datasources.get(name)
            && let Some(engine) = datasource.get("engine")
            && let Some(engine) = engine.as_str()
        {
            return Ok(engine);
        }
        MetadataError::metadata_load_from_file(
            name,
            "selected datasource not found or engine not specified",
        )
    }

    /// Resolve normal analysis metadata for a selected implemented setup task.
    ///
    /// # Errors
    /// Rejects missing or invalid selected datasource metadata.
    pub fn selected_metadata(&self, name: &str) -> Result<SqlFunMetadata, MetadataError> {
        let Some(value) = self.datasources.get(name) else {
            return MetadataError::metadata_load_from_file(name, "selected datasource not found");
        };
        SqlFunMetadata::from_value(name, value, &self.metadata_file)
    }
}

#[cfg(test)]
mod test_initialization_candidates {
    use crate::{
        metadata::InitializationDatasources, test_helpers::initialization_choices_project,
    };

    #[rstest::rstest]
    #[case::connector_only("connector = 'tokio-postgres'", &[])]
    #[case::scalar_settings("connector = 'tokio-postgres'\nengine = 'PostgreSQL'\nversion = '17'", &[])]
    #[case::named_with_setting("connector = 'tokio-postgres'\n[package.metadata.database.main]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '17'", &["main"])]
    fn settings_are_not_datasource_candidates(#[case] declarations: &str, #[case] names: &[&str]) {
        let project = initialization_choices_project(declarations);
        let choices =
            InitializationDatasources::load_from(&project.path().join("Cargo.toml")).unwrap();
        assert_eq!(choices.names(), names);
    }
}

/// Metadata collection
#[derive(Default)]
pub struct SqlFunMetadataCollection(HashMap<String, SqlFunMetadata>);

impl SqlFunMetadataCollection {
    /// insert metadata to collection
    pub fn insert(&mut self, value: SqlFunMetadata) -> Option<SqlFunMetadata> {
        self.0.insert(String::from(value.name()), value)
    }

    /// get named metadata
    #[must_use]
    pub fn get(&self, datasource_name: &str) -> Option<&SqlFunMetadata> {
        self.0.get(datasource_name)
    }

    /// remove and return named metadata
    pub fn remove(&mut self, datasource_name: &str) -> Option<SqlFunMetadata> {
        self.0.remove(datasource_name)
    }

    /// load form TOML.
    ///
    /// - source toml requires Cargo.toml like structure.
    /// - this function retrieves package.metadata.database and deserialize metadata
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError`] if the file cannot be read or parsed.
    #[tracing::instrument(err)]
    pub fn load_from<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Result<Self, MetadataError> {
        tracing::info!("loading metadata from {path:?}");
        let file_text = std::fs::read_to_string(&path)?;

        Self::from_str(&file_text, path)
    }

    /// load from `Cargo.toml` like text
    pub(crate) fn from_str<P: AsRef<Path> + std::fmt::Debug>(
        source: &str,
        path_hint: P,
    ) -> Result<Self, MetadataError> {
        use MetadataError as Error;
        let cargo_metadata: toml::Value = toml::from_str(source)?;
        let Some(package) = cargo_metadata.get("package") else {
            Error::metadata_load_from_file(
                &path_hint,
                &format!("package not defined in {path_hint:?}"),
            )?
        };
        let Some(metadata) = package.get("metadata") else {
            Error::metadata_load_from_file(
                &path_hint,
                &format!("package.metadata not defined in {path_hint:?}"),
            )?
        };
        let Some(database) = metadata.get("database") else {
            Error::metadata_load_from_file(
                &path_hint,
                &format!("package.metadata.database not defined in {path_hint:?}"),
            )?
        };

        let Some(datasources) = database.as_table() else {
            Error::metadata_load_from_file(
                &path_hint,
                &format!("package.metadata.database requires table {path_hint:?}"),
            )?
        };
        let mut result = Self::default();
        for (key, value) in datasources {
            if let Some(_existing) =
                result.insert(SqlFunMetadata::from_value(key, value, path_hint.as_ref())?)
            {
                Error::metadata_load_from_file(
                    &path_hint,
                    &format!("duplicate datasource name {key}"),
                )?;
            }
        }
        Ok(result)
    }
}

/// engine version for `PostgreSQL`
#[derive(Debug, Clone, PartialEq)]
pub struct PostgresEngineVersion(Either<PostgresVersion, String>);

impl<'de> serde::Deserialize<'de> for PostgresEngineVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s)
            .map_err(|e| serde::de::Error::custom(format!("invalid value for EngineVersion {e}")))
    }
}

impl FromStr for PostgresEngineVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(pg_ver) = PostgresVersion::from_str(s) {
            Ok(Self(Either::Left(pg_ver)))
        } else {
            // TODO validate custom version string
            Ok(Self(Either::Right(s.to_string())))
        }
    }
}

impl std::fmt::Display for PostgresEngineVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Either::Left(v) => write!(f, "{v}"),
            Either::Right(v) => write!(f, "{v}"),
        }
    }
}

impl PostgresEngineVersion {
    /// Whether this uses canonical `PostgreSQL` major/prerelease naming.
    ///
    /// This does not imply that builtin extraction is supported for the version.
    #[must_use]
    pub fn is_wellknown(&self) -> bool {
        self.0.is_left()
    }

    /// get definition base path
    pub fn definition_base_path<P: AsRef<Path>>(&self, home: P) -> PathBuf {
        match &self.0 {
            Either::Left(version) => version.definition_base_path(home.as_ref()),
            Either::Right(custom) => home.as_ref().join("postgres").join("custom").join(custom),
        }
    }
}

#[cfg(test)]
mod test_postgres_engine_version_paths {
    use crate::metadata::PostgresEngineVersion;
    use std::path::{Path, PathBuf};

    #[rstest::rstest]
    #[case::future_stable("19", "home/postgres/19")]
    #[case::prerelease("19beta1", "home/postgres/19beta1")]
    #[case::custom("fixture", "home/postgres/custom/fixture")]
    fn resolves_canonical_loader_paths(#[case] text: &str, #[case] expected: &str) {
        let version: PostgresEngineVersion = text.parse().unwrap();
        assert_eq!(
            version.definition_base_path(Path::new("home")),
            PathBuf::from(expected)
        );
    }
}

/// Well known database engine
#[derive(serde::Deserialize, Debug, Clone)]
pub enum WellKnownDbEngine {
    /// `PostgreSQL` Engine
    PostgreSQL,
}

impl FromStr for WellKnownDbEngine {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "PostgreSQL" {
            Ok(Self::PostgreSQL)
        } else {
            Err(s.to_string())
        }
    }
}

/// Database engine name
#[derive(Debug, Clone)]
pub struct DatabaseEngine(Either<WellKnownDbEngine, String>);

impl<'de> serde::Deserialize<'de> for DatabaseEngine {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.parse::<WellKnownDbEngine>() {
            Ok(wk) => Self(Either::Left(wk)),
            Err(_) => Self(Either::Right(s)),
        })
    }
}

impl DatabaseEngine {
    #[must_use]
    pub fn is_wellknown(&self) -> bool {
        self.0.is_left()
    }

    #[must_use]
    pub fn is_custom(&self) -> bool {
        self.0.is_right()
    }

    #[must_use]
    pub const fn as_wellknown(&self) -> Option<&WellKnownDbEngine> {
        match &self.0 {
            Either::Left(l) => Some(l),
            Either::Right(_) => None,
        }
    }

    #[must_use]
    pub const fn as_custom(&self) -> Option<&String> {
        match &self.0 {
            Either::Right(r) => Some(r),
            Either::Left(_) => None,
        }
    }
}

/// Validated exact catalog role names, ordered independently of input order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RoleInventory(std::collections::BTreeSet<String>);

impl<'de> serde::Deserialize<'de> for RoleInventory {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let names = <Vec<String> as serde::Deserialize>::deserialize(deserializer)?;
        let mut result = std::collections::BTreeSet::new();
        for name in names {
            if name.is_empty() || name.contains('\0') {
                return Err(serde::de::Error::custom(
                    "roles require nonempty names without NUL",
                ));
            }
            if !result.insert(name.clone()) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate role name: {name}"
                )));
            }
        }
        Ok(Self(result))
    }
}

impl RoleInventory {
    /// Convert exact names into the consumer's role definitions.
    #[must_use]
    pub fn definitions<T: for<'a> From<&'a str>>(&self) -> Vec<T> {
        self.0.iter().map(|name| T::from(name.as_str())).collect()
    }
}

/// `sql-fun` metadata structures
///
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SqlFunMetadata {
    #[serde(flatten)]
    codegen_options: CodegenOptions,

    #[serde(default)]
    rewrite_options: RewriteOptions,

    /// Datasource name
    #[serde(skip)]
    name: Option<String>,

    /// Database Engine
    engine: DatabaseEngine,

    /// SQL dialect
    dialect: SqlDialect,

    /// Database engine version
    #[serde(rename = "version")]
    engine_version: Option<PostgresEngineVersion>,

    /// Database extensions
    extensions: Option<PostgresExtensionsCollection>,

    /// Explicit `PostgreSQL` database identity for catalog-qualified SQL references.
    #[serde(
        rename = "database-name",
        default,
        deserialize_with = "SqlFunMetadata::deserialize_database_name"
    )]
    database_name: Option<String>,

    /// Explicit exact-name role inventory for offline ownership analysis.
    roles: Option<RoleInventory>,

    /// Schema search path
    search_path: Option<Vec<String>>,

    /// Explicit lint configuration file, relative to the metadata file.
    lint_configuration: Option<PathBuf>,

    /// CTE catalog directory path
    cte_catalog: Option<PathBuf>,

    /// Database connection dirver/midle-ware
    connector: Option<String>,
}

/// metadata related error
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    /// container structure failure
    #[error("sql-fun metadata can not load from {0}: {1}")]
    MetadataLoadFromFile(PathBuf, String),

    /// file io error
    #[error("metadata io error {0}")]
    Io(#[from] std::io::Error),

    /// metadata deserialization error
    #[error("metadata format error {0}")]
    Serde(#[from] toml::de::Error),

    /// CTE Catalog not existing
    #[error("CTE catalog not exists in {0}")]
    CteCatalogNotExist(PathBuf),
}

impl MetadataError {
    /// error in load metadata from file
    ///
    /// # Errors
    ///
    /// Always returns [`MetadataError::MetadataLoadFromFile`].
    pub fn metadata_load_from_file<T, P: AsRef<Path>>(path: P, message: &str) -> Result<T, Self> {
        Err(Self::MetadataLoadFromFile(
            path.as_ref().to_path_buf(),
            String::from(message),
        ))
    }

    /// CTE catalog not exist
    ///
    /// # Errors
    ///
    /// Always returns [`MetadataError::CteCatalogNotExist`].
    pub fn cte_catalog_not_exist<T, P: AsRef<Path>>(path: P) -> Result<T, Self> {
        Err(Self::CteCatalogNotExist(path.as_ref().to_path_buf()))
    }
}

impl SqlFunMetadata {
    /// Configured lint file resolved relative to the metadata file, without checking existence.
    #[must_use]
    pub fn lint_configuration(&self) -> Option<&Path> {
        self.lint_configuration.as_deref()
    }

    /// Load this datasource's lint definitions without semantic validation.
    ///
    /// # Errors
    /// Reports unreadable or undecodable configuration; missing files are empty.
    pub fn load_lint_configuration(&self) -> Result<LintConfigurations, LintConfigurationError> {
        let Some(path) = self.lint_configuration() else {
            return Ok(LintConfigurations::default());
        };
        LintConfigurations::load_from(path)
    }
}

#[cfg(test)]
mod test_lint_metadata {
    use crate::{
        InitializationDatasources, SqlFunMetadataCollection,
        test_helpers::{TestProjectDir, write_lint_metadata},
    };

    #[rstest::rstest]
    #[case::omitted(None)]
    #[case::relative(Some("lint.toml"))]
    fn resolves_selected_and_collection_paths(#[case] selection: Option<&str>) {
        let project = TestProjectDir::new().unwrap();
        let metadata_file = write_lint_metadata(&project, selection);
        let collection = SqlFunMetadataCollection::load_from(&metadata_file).unwrap();
        let selected = InitializationDatasources::load_from(&metadata_file)
            .unwrap()
            .selected_metadata("default")
            .unwrap();
        let expected = selection.map(|path| metadata_file.parent().unwrap().join(path));
        assert_eq!(
            collection.get("default").unwrap().lint_configuration(),
            expected.as_deref()
        );
        assert_eq!(selected.lint_configuration(), expected.as_deref());
        assert!(
            selected
                .load_lint_configuration()
                .unwrap()
                .deprecated()
                .is_empty()
        );
    }

    #[test]
    fn datasource_files_remain_independent() {
        let project = TestProjectDir::new().unwrap();
        let metadata_file = project
            .write_file(
                "config/Cargo.toml",
                r#"
[package.metadata.database.default]
engine = "PostgreSQL"
dialect = "postgres"
lint_configuration = "default.toml"
[package.metadata.database.other]
engine = "PostgreSQL"
dialect = "postgres"
lint_configuration = "other.toml"
"#,
            )
            .unwrap();
        project
            .write_file("config/other.toml", "invalid TOML")
            .unwrap();
        let collection = SqlFunMetadataCollection::load_from(&metadata_file).unwrap();
        assert!(
            collection
                .get("default")
                .unwrap()
                .load_lint_configuration()
                .unwrap()
                .deprecated()
                .is_empty()
        );
        assert!(
            collection
                .get("other")
                .unwrap()
                .load_lint_configuration()
                .is_err()
        );
        let selected = InitializationDatasources::load_from(&metadata_file)
            .unwrap()
            .selected_metadata("default")
            .unwrap();
        assert!(
            selected
                .load_lint_configuration()
                .unwrap()
                .deprecated()
                .is_empty()
        );
    }

    #[test]
    fn absolute_path_loads_only_on_request() {
        let project = TestProjectDir::new().unwrap();
        let lint = project.write_file("lint.toml", "not valid TOML").unwrap();
        let metadata_file = write_lint_metadata(&project, Some(lint.to_str().unwrap()));
        let declarations = InitializationDatasources::load_from(&metadata_file).unwrap();
        assert_eq!(declarations.names(), ["default"]);
        let metadata = declarations.selected_metadata("default").unwrap();
        assert_eq!(metadata.lint_configuration(), Some(lint.as_path()));
        assert!(metadata.load_lint_configuration().is_err());
        project
            .write_file(
                "lint.toml",
                r#"[[lint.deprecated]]
kind = "function"
name = ["public", "legacy"]"#,
            )
            .unwrap();
        assert_eq!(
            metadata.load_lint_configuration().unwrap().deprecated()[0].name(),
            ["public", "legacy"]
        );
    }
}

impl SqlFunMetadata {
    fn deserialize_database_name<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<String>, D::Error> {
        let name = <Option<String> as serde::Deserialize>::deserialize(deserializer)?;
        if name
            .as_ref()
            .is_some_and(|name| name.is_empty() || name.contains('\0'))
        {
            return Err(serde::de::Error::custom(
                "database-name must be a nonempty identifier without NUL",
            ));
        }
        Ok(name)
    }

    /// Explicit role inventory; omission differs from an explicitly empty list.
    #[must_use]
    pub const fn roles(&self) -> Option<&RoleInventory> {
        self.roles.as_ref()
    }

    /// Code generation options configured for this datasource.
    #[must_use]
    pub const fn codegen_options(&self) -> &CodegenOptions {
        &self.codegen_options
    }

    /// Executable SQL rewrite choices configured for this datasource.
    #[must_use]
    pub const fn rewrite_options(&self) -> &RewriteOptions {
        &self.rewrite_options
    }

    /// deserialize form TOML Value
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError`] when TOML conversion fails.
    pub(crate) fn from_value(
        name: &str,
        database: &toml::Value,
        metadata_file: &Path,
    ) -> Result<Self, MetadataError> {
        let mut db_metadata: Self = database.clone().try_into()?;
        db_metadata.name = Some(String::from(name));
        if let Some(path) = &mut db_metadata.lint_configuration
            && path.is_relative()
        {
            *path = metadata_file
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(&*path);
        }
        Ok(db_metadata)
    }

    /// get datasource name
    #[must_use]
    pub const fn name(&self) -> &str {
        if let Some(name) = &self.name {
            name.as_str()
        } else {
            "default"
        }
    }

    /// Explicit database identity, independent of the datasource selection name.
    #[must_use]
    pub fn database_name(&self) -> Option<&str> {
        self.database_name.as_deref()
    }

    /// get CTE catalog directory path
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError`] if the catalog path does not exist.
    pub fn cte_catalog(&self) -> Result<Option<&PathBuf>, MetadataError> {
        let Some(cte_catalog) = &self.cte_catalog else {
            return Ok(None);
        };
        if cte_catalog.exists() {
            Ok(Some(cte_catalog))
        } else {
            MetadataError::cte_catalog_not_exist(cte_catalog)?
        }
    }

    /// get lockfile path
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] if the context error.
    pub fn lockfile_path(&self, context: &SqlFunArgs) -> Result<PathBuf, SqlFunArgsError> {
        context.lockfile_path(self.name())
    }

    /// get schema dump path
    ///
    /// # Errors
    ///
    /// Returns an error if the metadata file cannot be located.
    pub fn schema_dump_path(&self, context: &SqlFunArgs) -> Result<PathBuf, SqlFunArgsError> {
        context.schema_dump_path(self.name())
    }

    /// get SQL dialect
    #[must_use]
    pub const fn sql_dialect(&self) -> SqlDialect {
        self.dialect
    }

    /// get db engine
    #[must_use]
    pub const fn engine(&self) -> &DatabaseEngine {
        &self.engine
    }

    /// get engine version
    ///
    ///  this function returns just metadata defined.
    ///
    /// See [`crate::SqlFunArgs::postgres_version`] for argument precedence.
    ///
    #[must_use]
    pub const fn engine_version(&self) -> &Option<PostgresEngineVersion> {
        &self.engine_version
    }

    /// get search path
    ///
    ///  this function returns just metadata defined.
    ///
    /// See [`crate::SqlFunArgs::postgres_search_path`] for argument precedence.
    ///
    #[must_use]
    pub const fn search_path(&self) -> &Option<Vec<String>> {
        &self.search_path
    }

    /// get `PostgreSQL` extensions allowed
    #[must_use]
    pub const fn postgres_extensions(&self) -> &Option<PostgresExtensionsCollection> {
        &self.extensions
    }

    /// get database connection driver / middle-ware
    #[must_use]
    pub const fn connector(&self) -> &Option<String> {
        &self.connector
    }
}

#[cfg(test)]
mod test_rewrite_metadata {
    use crate::test_helpers::metadata_with_codegen_options;

    #[rstest::rstest]
    #[case::omitted("", None)]
    #[case::empty("[package.metadata.database.default.rewrite_options]", None)]
    #[case::disabled("rewrite_options = { expand_asterisk = false }", Some(false))]
    #[case::enabled("rewrite_options = { expand_asterisk = true }", Some(true))]
    #[case::nested(
        "[package.metadata.database.default.rewrite_options]\nexpand_asterisk = true",
        Some(true)
    )]
    fn retains_explicit_rewrite_choice(#[case] input: &str, #[case] expected: Option<bool>) {
        let metadata = metadata_with_codegen_options(input).unwrap();
        let selected = metadata.get("default").unwrap();
        assert_eq!(selected.rewrite_options().expand_asterisk(), expected);
        assert!(selected.codegen_options().builder_setter().is_mutable());
    }

    #[rstest::rstest]
    #[case::scalar("rewrite_options = true")]
    #[case::string(r#"rewrite_options = { expand_asterisk = "true" }"#)]
    #[case::number("rewrite_options = { expand_asterisk = 1 }")]
    fn rejects_invalid_rewrite_options(#[case] input: &str) {
        assert!(metadata_with_codegen_options(input).is_err());
    }
}

#[cfg(test)]
mod test_database_name {
    use crate::test_helpers::metadata_with_codegen_options;

    #[rstest::rstest]
    #[case::absent("", None)]
    #[case::explicit("database-name = 'my.db'", Some("my.db"))]
    #[case::case_sensitive("database-name = 'My DB'", Some("My DB"))]
    fn retains_only_explicit_identity(#[case] input: &str, #[case] expected: Option<&str>) {
        let metadata = metadata_with_codegen_options(input).unwrap();
        let selected = metadata.get("default").unwrap();
        assert_eq!(selected.database_name(), expected);
        assert_eq!(selected.name(), "default");
    }

    #[rstest::rstest]
    #[case::empty("database-name = ''")]
    #[case::wrong_type("database-name = 42")]
    #[case::nul(r#"database-name = "bad\u0000name""#)]
    fn rejects_invalid_identity(#[case] input: &str) {
        assert!(metadata_with_codegen_options(input).is_err());
    }
}

#[cfg(test)]
mod test_codegen_metadata {
    use crate::test_helpers::metadata_with_codegen_options;

    #[test]
    fn flat_metadata_supports_all_contracts() {
        for setter in ["mutable", "consuming"] {
            for fallible in [false, true] {
                let metadata = metadata_with_codegen_options(&format!(
                    "builder_setter = {setter:?}\nbuilder_build_fallible = {fallible}"
                ))
                .unwrap();
                let options = metadata.get("default").unwrap().codegen_options();
                assert_eq!(options.builder_setter().is_mutable(), setter == "mutable");
                assert_eq!(options.builder_build_fallible(), fallible);
            }
        }
    }

    #[test]
    fn omitted_fields_keep_independent_defaults() {
        for input in [
            "",
            "builder_setter = \"consuming\"",
            "builder_build_fallible = false",
        ] {
            let metadata = metadata_with_codegen_options(input).unwrap();
            let options = metadata.get("default").unwrap().codegen_options();
            assert_eq!(
                options.builder_setter().is_mutable(),
                !input.contains("consuming")
            );
            assert_eq!(options.builder_build_fallible(), !input.contains("false"));
        }
    }

    #[test]
    fn invalid_contracts_are_rejected() {
        for input in [
            "builder_setter = \"unknown\"",
            "builder_setter = false",
            "builder_build_fallible = \"false\"",
        ] {
            assert!(metadata_with_codegen_options(input).is_err());
        }
    }
}

#[cfg(test)]
mod test_role_metadata {
    use crate::metadata::{InitializationDatasources, RoleInventory};

    use crate::test_helpers::{initialization_choices_project, metadata_with_codegen_options};

    #[rstest::rstest]
    #[case::omitted("", None)]
    #[case::empty("roles = []", Some(vec![]))]
    #[case::exact_names(r#"roles = ['Owner', 'owner', 'a.b', '所有者', 'a"b', ' spaced ']"#,
        Some(vec![" spaced ", "Owner", "a\"b", "a.b", "owner", "所有者"]))]
    fn preserves_explicit_inventory(#[case] input: &str, #[case] expected: Option<Vec<&str>>) {
        let metadata = metadata_with_codegen_options(input).unwrap();
        let selected = metadata.get("default").unwrap();
        let names = selected.roles().map(RoleInventory::definitions::<String>);
        assert_eq!(
            names,
            expected.map(|names| names.into_iter().map(String::from).collect())
        );
    }

    #[rstest::rstest]
    #[case::scalar("roles = 'owner'")]
    #[case::table("roles = { owner = true }")]
    #[case::number("roles = [42]")]
    #[case::empty_name("roles = ['']")]
    #[case::nul(r#"roles = ["bad\u0000name"]"#)]
    #[case::duplicate("roles = ['owner', 'owner']")]
    fn rejects_invalid_inventory_in_both_loaders(#[case] input: &str) {
        assert!(metadata_with_codegen_options(input).is_err());
        let project = initialization_choices_project(&format!(
            "[package.metadata.database.app]\nengine = 'PostgreSQL'\ndialect = 'postgres'\n{input}"
        ));
        let declarations =
            InitializationDatasources::load_from(&project.path().join("Cargo.toml")).unwrap();
        assert!(declarations.selected_metadata("app").is_err());
    }

    #[test]
    fn selected_loader_does_not_decode_unselected_role_inventory() {
        let project = initialization_choices_project(
            "[package.metadata.database.app]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nroles = ['owner']\n[package.metadata.database.other]\nroles = 42",
        );
        let declarations =
            InitializationDatasources::load_from(&project.path().join("Cargo.toml")).unwrap();
        let selected = declarations.selected_metadata("app").unwrap();
        assert_eq!(selected.roles().unwrap().definitions::<String>(), ["owner"]);
        assert!(declarations.selected_metadata("other").is_err());
    }
}
