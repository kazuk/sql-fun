use std::{collections::HashMap, path::PathBuf, str::FromStr};

use crate::{
    PostgresExtensionsCollection, SqlFunArgs, SqlFunArgsError, SqlFunMetadata,
    metadata::EngineVersion,
};

#[derive(clap::Args, Debug, serde::Serialize, serde::Deserialize, Clone)]

pub struct PostgresArgs {
    /// `PostgreSQL` search path
    #[clap(long("pg_search_path"), env = Self::SQL_FUN_PG_SEARCH_PATH)]
    postgres_search_path: Option<Vec<String>>,

    /// `PostgreSQL` version for built-in schema
    #[clap(long, env = Self::SQL_FUN_POSTGRES_VERSION)]
    postgres_version: Option<String>,

    /// Comma-separated Postgres extensions (name[@version])
    #[clap(long, env = Self::SQL_FUN_EXTENSIONS )]
    postgres_extensions: Option<String>,

    /// Built-in schema directory (overrides version)
    #[clap(long, env = Self::SQL_FUN_BUITIN_SCHEMA)]
    builtin_schema_dir: Option<PathBuf>,

    /// Directory for Postgres extension SQL files
    #[clap(long, env = Self::SQL_FUN_EXTENSIONS_DIR)]
    postgres_extensions_dir: Option<PathBuf>,
}

impl PostgresArgs {
    const SQL_FUN_PG_SEARCH_PATH: &str = "SQL_FUN_PG_SEARCH_PATH";
    const SQL_FUN_POSTGRES_VERSION: &str = "SQL_FUN_POSTGRES_VERSION";
    const SQL_FUN_EXTENSIONS: &str = "SQL_FUN_EXTENSIONS";
    const SQL_FUN_BUITIN_SCHEMA: &str = "SQL_FUN_BUITIN_SCHEMA";
    const SQL_FUN_EXTENSIONS_DIR: &str = "SQL_FUN_EXTENSIONS_DIR";

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
    ) -> Result<EngineVersion, SqlFunArgsError> {
        let Some(version) = &self.postgres_version else {
            let Some(engine_version) = metadata.engine_version() else {
                SqlFunArgsError::no_postgres_engine_version_specified()?
            };
            return Ok(engine_version.clone());
        };
        let Ok(version) = EngineVersion::from_str(version) else {
            SqlFunArgsError::invalid_engine_version(version)?
        };
        Ok(version)
    }

    /// get postgres extensions
    pub fn postgres_extensions(
        &self,
        metadata: &SqlFunMetadata,
    ) -> Result<PostgresExtensionsCollection, SqlFunArgsError> {
        if let Some(extensions) = &self.postgres_extensions {
            let parse_result = PostgresExtensionsCollection::from_str(extensions);
            match parse_result {
                Ok(v) => Ok(v),
                Err((msg, source)) => SqlFunArgsError::parsing_postgres_extensions(&msg, &source),
            }
        } else if let Some(extensions) = metadata.postgres_extensions() {
            Ok(extensions.clone())
        } else {
            Ok(PostgresExtensionsCollection::default())
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
        let extensions = &mut self.postgres_extensions(metadata)?;
        extensions.resolve_versions(&extensions_dir)?;

        envs.insert(
            Self::SQL_FUN_EXTENSIONS_DIR.to_string(),
            extensions_dir.to_string_lossy().to_string(),
        );
        envs.insert(Self::SQL_FUN_EXTENSIONS.to_string(), extensions.to_string());
        envs.insert(
            Self::SQL_FUN_BUITIN_SCHEMA.to_string(),
            args.builtin_info_dir(metadata)?
                .to_string_lossy()
                .to_string(),
        );

        Ok(())
    }
}
