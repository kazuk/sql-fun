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
pub struct InitializationDatasources(toml::Table);

impl InitializationDatasources {
    /// Read datasource declarations without choosing or initializing an engine.
    ///
    /// # Errors
    /// Rejects unreadable or malformed project metadata.
    pub fn load_from(path: &Path) -> Result<Self, MetadataError> {
        let text = std::fs::read_to_string(path)?;
        let document: toml::Value = toml::from_str(&text)?;
        let Some(database) = document
            .get("package")
            .and_then(|value| value.get("metadata"))
            .and_then(|value| value.get("database"))
        else {
            return Ok(Self(toml::Table::new()));
        };
        let Some(table) = database.as_table() else {
            return MetadataError::metadata_load_from_file(
                path,
                "package.metadata.database requires a table",
            );
        };
        Ok(Self(table.clone()))
    }

    /// Datasource names in deterministic prompt order.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names = self
            .0
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
        let Some(engine) = self
            .0
            .get(name)
            .and_then(|value| value.get("engine"))
            .and_then(toml::Value::as_str)
        else {
            return MetadataError::metadata_load_from_file(
                name,
                "selected datasource not found or engine not specified",
            );
        };
        Ok(engine)
    }

    /// Resolve normal analysis metadata for a selected implemented setup task.
    ///
    /// # Errors
    /// Rejects missing or invalid selected datasource metadata.
    pub fn selected_metadata(&self, name: &str) -> Result<SqlFunMetadata, MetadataError> {
        let Some(value) = self.0.get(name) else {
            return MetadataError::metadata_load_from_file(name, "selected datasource not found");
        };
        SqlFunMetadata::from_value(name, value)
    }
}

#[cfg(test)]
mod test_initialization_candidates {
    use super::InitializationDatasources;
    use crate::test_helpers::initialization_choices_project;

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
            if let Some(_existing) = result.insert(SqlFunMetadata::from_value(key, value)?) {
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
    use super::PostgresEngineVersion;
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

/// `sql-fun` metadata structures
///
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SqlFunMetadata {
    #[serde(flatten)]
    codegen_options: crate::CodegenOptions,

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

    /// Schema search path
    search_path: Option<Vec<String>>,

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
    /// Code generation options configured for this datasource.
    #[must_use]
    pub const fn codegen_options(&self) -> &crate::CodegenOptions {
        &self.codegen_options
    }

    /// deserialize form TOML Value
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError`] when TOML conversion fails.
    pub(crate) fn from_value(name: &str, database: &toml::Value) -> Result<Self, MetadataError> {
        let mut db_metadata: Self = database.clone().try_into()?;
        db_metadata.name = Some(String::from(name));
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
