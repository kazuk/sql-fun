use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{SqlDialect, SqlFunArgsError, SqlFunMetadata};

fn path_owner<P: AsRef<Path>>(path: P) -> Result<u32, std::io::Error> {
    let meta = std::fs::metadata(path)?;
    Ok(meta.uid())
}

#[derive(clap::Args, Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct MetadataArgs {
    /// home_dir
    #[clap(long, env = Self::SQL_FUN_HOME)]
    sql_fun_home: Option<PathBuf>,

    /// Metadata file format (TOML)
    #[clap(long, env = Self::SQL_FUN_METADATA_FORMAT, default_value = "TOML")]
    metadata_format: String,

    /// Path to metadata file (Cargo.toml)
    #[clap(long, env = Self::SQL_FUN_METADATA_FILE)]
    metadata_file: Option<String>,

    /// Schema selector
    #[clap(long, env = Self::SQL_FUN_SCHEMA, default_value = "develop")]
    schema: String,

    /// Schema dump file (overrides --schema)
    #[clap(long, env = Self::SQL_FUN_SCHEMA_FILE )]
    schema_file: Option<PathBuf>,

    /// SQL dialect (e.g., postgresql)
    #[clap(long, env = Self::SQL_FUN_DIALECT)]
    sql_dialect: Option<String>,

    /// Directory for CTE catalog
    #[clap(long, env = Self::SQL_FUN_CTE_CATALOG_DIR)]
    cte_catalog_dir: Option<PathBuf>,
}

impl MetadataArgs {
    const SQL_FUN_HOME: &str = "SQL_FUN_HOME";
    const SQL_FUN_METADATA_FORMAT: &str = "SQL_FUN_METADATA_FORMAT";
    const SQL_FUN_METADATA_FILE: &str = "SQL_FUN_METADATA_FILE";
    const SQL_FUN_SCHEMA: &str = "SQL_FUN_SCHEMA";
    const SQL_FUN_SCHEMA_FILE: &str = "SQL_FUN_SCHEMA_FILE";
    const SQL_FUN_DIALECT: &str = "SQL_FUN_DIALECT";
    const SQL_FUN_CTE_CATALOG_DIR: &str = "SQL_FUN_CTE_CATALOG_DIR";

    /// get home dir path
    ///
    /// If `--sql-fun-home` or `SQL_FUN_HOME` env present : configured value
    /// Else returns `${HOME}/.sql-fun`
    ///
    pub fn sql_fun_home(&self) -> Result<PathBuf, SqlFunArgsError> {
        if let Some(home_arg) = &self.sql_fun_home {
            let result = if home_arg.is_relative() {
                std::env::current_dir()?.join(home_arg)
            } else {
                home_arg.clone()
            };
            if result.exists() {
                Ok(result.canonicalize()?)
            } else {
                SqlFunArgsError::home_not_exist(result)?
            }
        } else if let Some(home) = std::env::home_dir() {
            Ok(home.join(".sql-fun"))
        } else {
            SqlFunArgsError::user_home_not_defined()?
        }
    }

    /// gets metadata format value in args
    pub fn metadata_format(&self) -> &str {
        &self.metadata_format
    }

    /// return raw value for `--metadata-file`
    pub fn metadata_file_raw(&self) -> Result<&str, SqlFunArgsError> {
        if self.metadata_file.is_none() {
            if self.metadata_format() == "TOML" {
                Ok("Cargo.toml")
            } else {
                Err(SqlFunArgsError::NotSupported(
                    String::from("--metadata-format"),
                    String::from(self.metadata_format()),
                ))
            }
        } else {
            Ok(self.metadata_file.as_ref().unwrap())
        }
    }

    /// search metadata file with same-owner policy
    pub fn metadata_file(&self) -> Result<PathBuf, SqlFunArgsError> {
        let raw_arg = PathBuf::from(self.metadata_file_raw()?);
        if raw_arg.is_absolute() {
            if raw_arg.exists() {
                Ok(raw_arg)
            } else {
                SqlFunArgsError::file_path_not_found(raw_arg)?
            }
        } else {
            let current_dir = std::env::current_dir()?;
            let owner = path_owner(&current_dir)?;
            log::trace!("searching {raw_arg:?} with owner {owner}");
            for ancestor in current_dir.ancestors() {
                let dir_owner = path_owner(ancestor)?;
                if owner != dir_owner {
                    log::error!("owner not match for {ancestor:?}");
                    break;
                }
                let mut file = ancestor.to_path_buf();
                file.push(&raw_arg);
                if file.exists() {
                    let file_owner = path_owner(&file)?;
                    if owner == file_owner {
                        return Ok(file);
                    } else {
                        log::error!("owner not match for {file:?}");
                        break;
                    }
                } else {
                    log::trace!("{file:?} not exist");
                }
            }
            SqlFunArgsError::file_path_not_found(raw_arg)?
        }
    }

    /// get schema file
    pub fn schema_file(&self, metadata_file: &PathBuf) -> Result<PathBuf, SqlFunArgsError> {
        if let Some(schema_file) = &self.schema_file {
            if schema_file.exists() {
                Ok(schema_file.clone())
            } else {
                Err(SqlFunArgsError::FilePathNotFound(schema_file.clone()))?
            }
        } else {
            let schema = &self.schema;
            let Some(schema_dir) = metadata_file.parent() else {
                Err(SqlFunArgsError::Unexpected(format!(
                    "unexpected metadata_file.parent() is none: {metadata_file:?}"
                )))?
            };
            let schema_file = schema_dir.join(format!("sqlfun.{schema}.sql"));

            if schema_file.exists() {
                Ok(schema_file)
            } else {
                Err(SqlFunArgsError::FilePathNotFound(schema_file))?
            }
        }
    }

    /// get SQL dialect
    pub fn sql_dialect(&self, metadata: &SqlFunMetadata) -> Result<SqlDialect, SqlFunArgsError> {
        let Some(dialect) = &self.sql_dialect else {
            return Ok(metadata.sql_dialect());
        };
        let Ok(dialect) = SqlDialect::from_str(dialect) else {
            SqlFunArgsError::not_suppoted(dialect, "SQL Dialect")?
        };
        Ok(dialect)
    }

    /// get CTE catalog directory
    pub fn cte_catalog_dir(
        &self,
        metadata: &SqlFunMetadata,
    ) -> Result<Option<PathBuf>, SqlFunArgsError> {
        if let Some(cte_catalog) = &self.cte_catalog_dir {
            Ok(Some(cte_catalog.clone()))
        } else if let Some(cte_catalog) = metadata.cte_catalog()? {
            Ok(Some(cte_catalog.clone()))
        } else {
            Ok(None)
        }
    }

    pub fn get_envs(
        &self,
        envs: &mut HashMap<String, String>,
        metadata: &SqlFunMetadata,
    ) -> Result<(), SqlFunArgsError> {
        let metadata_file = self.metadata_file()?;

        insert_to_result(
            envs,
            Self::SQL_FUN_HOME,
            &self.sql_fun_home()?.to_string_lossy(),
        );
        insert_to_result(envs, Self::SQL_FUN_METADATA_FORMAT, self.metadata_format());

        insert_to_result(
            envs,
            Self::SQL_FUN_METADATA_FILE,
            &metadata_file.to_string_lossy(),
        );
        insert_to_result(envs, Self::SQL_FUN_SCHEMA, &self.schema);
        insert_to_result(
            envs,
            Self::SQL_FUN_SCHEMA_FILE,
            &self.schema_file(&metadata_file)?.to_string_lossy(),
        );
        insert_to_result(
            envs,
            Self::SQL_FUN_DIALECT,
            &self.sql_dialect(metadata)?.to_string(),
        );
        if let Some(cte_catalog_dir) = self.cte_catalog_dir(metadata)? {
            insert_to_result(
                envs,
                Self::SQL_FUN_CTE_CATALOG_DIR,
                &cte_catalog_dir.to_string_lossy(),
            );
        }
        return Ok(());

        fn insert_to_result(result: &mut HashMap<String, String>, name: &str, value: &str) {
            result.insert(name.to_string(), value.to_string());
        }
    }
}
