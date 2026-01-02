use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use either::Either;

use crate::{PostgresExtensionsCollection, PostgresVersion, SqlDialect};

#[derive(Debug, Clone, PartialEq)]
pub struct EngineVersion(Either<PostgresVersion, String>);

impl<'de> serde::Deserialize<'de> for EngineVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s)
            .map_err(|e| serde::de::Error::custom(format!("invalid value for EngineVersion {e}")))
    }
}

impl FromStr for EngineVersion {
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

impl std::fmt::Display for EngineVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Either::Left(v) => write!(f, "{v}"),
            Either::Right(v) => write!(f, "{v}"),
        }
    }
}

impl EngineVersion {
    pub fn is_wellknown(&self) -> bool {
        self.0.is_left()
    }

    pub fn definition_base_path<P: AsRef<Path>>(&self, home: P) -> PathBuf {
        if self.is_wellknown() {
            home.as_ref()
                .join("postgres")
                .join(Path::new(&self.to_string()))
        } else {
            home.as_ref()
                .join("postgres")
                .join("custom")
                .join(self.to_string())
        }
    }
}

/// Well known database engine
#[derive(serde::Deserialize, Debug)]
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
#[derive(Debug)]
pub struct DatabaseEngine(Either<WellKnownDbEngine, String>);

impl<'de> serde::Deserialize<'de> for DatabaseEngine {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.parse::<WellKnownDbEngine>() {
            Ok(wk) => DatabaseEngine(Either::Left(wk)),
            Err(_) => DatabaseEngine(Either::Right(s)),
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
    pub fn as_wellknown(&self) -> Option<&WellKnownDbEngine> {
        match &self.0 {
            Either::Left(l) => Some(l),
            Either::Right(_) => None,
        }
    }

    #[must_use]
    pub fn as_custom(&self) -> Option<&String> {
        match &self.0 {
            Either::Right(r) => Some(r),
            Either::Left(_) => None,
        }
    }
}

/// `sql-fun` metadata structures
///
#[derive(Debug, serde::Deserialize)]
pub struct SqlFunMetadata {
    /// Database Engine
    engine: DatabaseEngine,
    /// SQL dialect
    dialect: SqlDialect,
    /// Database engine version
    #[serde(rename = "version")]
    engine_version: Option<EngineVersion>,
    /// Database extensions
    extensions: Option<PostgresExtensionsCollection>,
    /// Schema search path
    search_path: Option<Vec<String>>,
    /// CTE catalog directory path
    cte_catalog: Option<PathBuf>,
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
    /// load form TOML.
    ///
    /// - source toml requires Cargo.toml like structure.
    /// - this function retrieves package.metadata.database and deserialize metadata
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError`] if the file cannot be read or parsed.
    pub fn load_from<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Result<Self, MetadataError> {
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
        Self::from_value(database)
    }

    /// deserialize form TOML Value
    ///
    /// # Errors
    ///
    /// Returns [`MetadataError`] when TOML conversion fails.
    pub(crate) fn from_value(database: &toml::Value) -> Result<Self, MetadataError> {
        let db_matadata: Self = database.clone().try_into()?;
        Ok(db_matadata)
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

    /// get SQL dialect
    #[must_use]
    pub fn sql_dialect(&self) -> SqlDialect {
        self.dialect
    }

    /// get db engine
    #[must_use]
    pub fn engine(&self) -> &DatabaseEngine {
        &self.engine
    }

    /// get engine version
    ///
    ///  this function returns just metadata defined.
    ///
    /// See [`crate::SqlFunArgs::postgres_version`] for argument precedence.
    ///
    #[must_use]
    pub fn engine_version(&self) -> &Option<EngineVersion> {
        &self.engine_version
    }

    /// get search path
    ///
    ///  this function returns just metadata defined.
    ///
    /// See [`crate::SqlFunArgs::postgres_search_path`] for argument precedence.
    ///  
    #[must_use]
    pub fn search_path(&self) -> &Option<Vec<String>> {
        &self.search_path
    }

    /// get `PostgreSQL` extensions allowed
    #[must_use]
    pub fn postgres_extensions(&self) -> &Option<PostgresExtensionsCollection> {
        &self.extensions
    }
}
