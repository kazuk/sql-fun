use std::{collections::HashMap, path::PathBuf, str::FromStr};

use crate::{
    PostgresExtensionsCollection, PostgresExtensionsParseError, SqlFunArgs, SqlFunArgsBuilder,
    SqlFunArgsBuilderError, SqlFunArgsError, SqlFunMetadata, metadata::PostgresEngineVersion,
};

#[derive(
    clap::Args, Debug, serde::Serialize, serde::Deserialize, Clone, derive_builder::Builder,
)]
#[builder(build_fn(error = "SqlFunArgsBuilderError"))]
pub struct PostgresArgs {
    /// `PostgreSQL` search path
    #[clap(long("pg_search_path"), env = Self::SQL_FUN_PG_SEARCH_PATH)]
    #[builder(setter(into, strip_option), default)]
    postgres_search_path: Option<Vec<String>>,

    /// `PostgreSQL` version for built-in schema
    #[clap(long, env = Self::SQL_FUN_POSTGRES_VERSION)]
    #[builder(setter(into, strip_option), default)]
    postgres_version: Option<String>,

    /// Comma-separated Postgres extensions (name[@version])
    #[clap(long, env = Self::SQL_FUN_EXTENSIONS, value_parser=PostgresExtensionsCollection::parse_arg )]
    #[builder(default, setter(strip_option))]
    postgres_extensions: Option<PostgresExtensionsCollection>,

    /// Built-in schema directory (overrides version)
    #[clap(long, env = Self::SQL_FUN_BUILTIN_SCHEMA)]
    #[builder(setter(into, strip_option), default)]
    builtin_schema_dir: Option<PathBuf>,

    /// Directory for Postgres extension SQL files
    #[clap(long, env = Self::SQL_FUN_EXTENSIONS_DIR)]
    #[builder(setter(into, strip_option), default)]
    postgres_extensions_dir: Option<PathBuf>,
}

impl SqlFunArgsBuilder {
    /// `PostgreSQL` search path
    pub fn postgres_search_path<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<Vec<String>>,
    {
        self.postgres_args.postgres_search_path(value);
        self
    }

    /// `PostgreSQL` search path from comma-separated string
    pub fn postgres_search_path_from_csv(&mut self, value: &str) -> &mut Self {
        let search_path = value
            .split(',')
            .filter(|item| !item.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if !search_path.is_empty() {
            self.postgres_search_path(search_path);
        }
        self
    }

    /// `PostgreSQL` version for built-in schema
    pub fn postgres_version<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<PostgresEngineVersion>,
    {
        let engine_version = value.into();
        self.postgres_args
            .postgres_version(format!("{engine_version}"));
        self
    }

    /// Comma-separated Postgres extensions (`name[@version]`)
    ///
    /// # Errors
    ///
    /// When parsing error returns reason and remains pair.
    ///
    pub fn postgres_extensions(
        &mut self,
        value: String,
    ) -> Result<&mut Self, PostgresExtensionsParseError> {
        let value = PostgresExtensionsCollection::try_from(value)?;
        self.postgres_args.postgres_extensions(value);
        Ok(self)
    }

    /// Built-in schema directory (overrides version)
    pub fn builtin_schema_dir<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<PathBuf>,
    {
        self.postgres_args.builtin_schema_dir(value);
        self
    }

    /// Directory for Postgres extension SQL files
    pub fn postgres_extensions_dir<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<PathBuf>,
    {
        self.postgres_args.postgres_extensions_dir(value);
        self
    }
}

impl PostgresArgs {
    const SQL_FUN_PG_SEARCH_PATH: &str = "SQL_FUN_PG_SEARCH_PATH";
    const SQL_FUN_POSTGRES_VERSION: &str = "SQL_FUN_POSTGRES_VERSION";
    const SQL_FUN_EXTENSIONS: &str = "SQL_FUN_EXTENSIONS";
    const SQL_FUN_BUILTIN_SCHEMA: &str = "SQL_FUN_BUILTIN_SCHEMA";
    const SQL_FUN_EXTENSIONS_DIR: &str = "SQL_FUN_EXTENSIONS_DIR";

    pub(crate) fn apply_envs(
        builder: &mut SqlFunArgsBuilder,
        envs: &HashMap<String, String>,
    ) -> Result<(), SqlFunArgsError> {
        if let Some(value) = envs.get(Self::SQL_FUN_PG_SEARCH_PATH) {
            builder.postgres_search_path_from_csv(value);
        }
        if let Some(value) = envs.get(Self::SQL_FUN_POSTGRES_VERSION) {
            let Ok(version) = PostgresEngineVersion::from_str(value) else {
                return SqlFunArgsError::invalid_engine_version(value);
            };
            builder.postgres_version(version);
        }
        if let Some(value) = envs.get(Self::SQL_FUN_EXTENSIONS).cloned()
            && let Err(err) = builder.postgres_extensions(value.clone())
        {
            return SqlFunArgsError::parsing_postgres_extensions(&err.to_string(), &value);
        }
        if let Some(value) = envs.get(Self::SQL_FUN_BUILTIN_SCHEMA) {
            builder.builtin_schema_dir(PathBuf::from(value));
        }
        if let Some(value) = envs.get(Self::SQL_FUN_EXTENSIONS_DIR) {
            builder.postgres_extensions_dir(PathBuf::from(value));
        }
        Ok(())
    }

    /// get `PostgreSQL` search path
    #[allow(clippy::unnecessary_wraps)]
    pub fn postgres_search_path(
        &self,
        metadata: &SqlFunMetadata,
    ) -> Result<Vec<String>, SqlFunArgsError> {
        if let Some(search_path) = &self.postgres_search_path {
            Ok(search_path.clone())
        } else if let Some(search_path) = metadata.search_path() {
            Ok(search_path.clone())
        } else {
            // default search path
            Ok(vec![String::from("public")])
        }
    }

    /// get `PostgreSQL` version
    pub fn postgres_version(
        &self,
        metadata: &SqlFunMetadata,
    ) -> Result<PostgresEngineVersion, SqlFunArgsError> {
        let Some(version) = &self.postgres_version else {
            let Some(engine_version) = metadata.engine_version() else {
                SqlFunArgsError::no_postgres_engine_version_specified()?
            };
            return Ok(engine_version.clone());
        };
        let Ok(version) = PostgresEngineVersion::from_str(version) else {
            SqlFunArgsError::invalid_engine_version(version)?
        };
        Ok(version)
    }

    /// get postgres extensions
    pub fn postgres_extensions(&self, metadata: &SqlFunMetadata) -> PostgresExtensionsCollection {
        if let Some(extensions) = &self.postgres_extensions {
            extensions.clone()
        } else if let Some(extensions) = metadata.postgres_extensions() {
            extensions.clone()
        } else {
            PostgresExtensionsCollection::default()
        }
    }

    pub fn builtin_schema_dir(&self) -> Option<&PathBuf> {
        self.builtin_schema_dir.as_ref()
    }

    pub fn postgres_extensions_dir(&self) -> Option<&PathBuf> {
        self.postgres_extensions_dir.as_ref()
    }

    pub fn get_envs(
        &self,
        envs: &mut HashMap<String, String>,
        args: &SqlFunArgs,
        metadata: &SqlFunMetadata,
    ) -> Result<(), SqlFunArgsError> {
        envs.insert(
            Self::SQL_FUN_PG_SEARCH_PATH.to_string(),
            self.postgres_search_path(metadata)?.join(","),
        );
        envs.insert(
            Self::SQL_FUN_POSTGRES_VERSION.to_string(),
            self.postgres_version(metadata)?.to_string(),
        );

        let extensions_dir = args.postgres_extensions_dir(metadata)?;
        let extensions = &mut self.postgres_extensions(metadata);
        extensions.resolve_versions(&extensions_dir)?;

        envs.insert(
            Self::SQL_FUN_EXTENSIONS_DIR.to_string(),
            extensions_dir.to_string_lossy().to_string(),
        );
        envs.insert(Self::SQL_FUN_EXTENSIONS.to_string(), extensions.to_string());
        envs.insert(
            Self::SQL_FUN_BUILTIN_SCHEMA.to_string(),
            args.builtin_info_dir(metadata)?
                .to_string_lossy()
                .to_string(),
        );

        Ok(())
    }
}
