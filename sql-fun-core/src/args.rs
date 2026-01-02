mod cli_sub_command;
mod error;
mod impl_metadata;
mod impl_sql_fun_home;
mod init_args;
mod log_args;
mod metadata_args;
mod postgres;

use std::{collections::HashMap, fmt::Display, path::PathBuf};

use clap::Parser;

use self::log_args::LoggingArgs;
pub use self::{
    cli_sub_command::CliSubCommand, error::SqlFunArgsError, init_args::InitializeArgs,
    metadata_args::MetadataArgs, postgres::PostgresArgs,
};

use crate::{PostgresExtensionsCollection, SqlDialect, SqlFunMetadata, metadata::EngineVersion};

#[derive(Debug)]
pub enum ConfigurationKind {
    EngineVersion,
}

impl Display for ConfigurationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigurationKind::EngineVersion => write!(f, "Database engine version"),
        }
    }
}

/// Common `sql-fun` CLI arguments and environment variables
#[derive(clap::Parser, Debug, serde::Serialize, serde::Deserialize, Clone)]
#[clap(name = "sql-fun", version, about, long_about = None)]
pub struct SqlFunArgs {
    #[command(flatten)]
    metadata_args: MetadataArgs,

    #[command(flatten)]
    postgres_args: PostgresArgs,

    #[command(flatten)]
    log_args: LoggingArgs,

    /// Output format (text or json)
    #[clap(long, env = Self::SQL_FUN_OUTPUT_FORMAT, default_value = "text")]
    output_format: String,

    /// Verbose mode (equivalent to --log-level=debug)
    #[clap(long)]
    verbose: bool,

    /// Disable loading built-in tables/views
    #[clap(long, env = Self::SQL_FUN_NO_BUILTIN)]
    no_builtin_tables: bool,

    /// Subcommand to execute (internal or external)
    #[clap(subcommand)]
    #[serde(skip)]
    command: CliSubCommand,
}

impl SqlFunArgs {
    /// returns true for builtin subcommand
    #[must_use]
    pub fn is_builtin(&self) -> bool {
        matches!(self.command, CliSubCommand::Initialize(_))
    }

    /// get [`CliSubCommand`]
    #[must_use]
    pub fn cli_sub_command(&self) -> &CliSubCommand {
        &self.command
    }

    /// get SQL dialect
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when dialect resolution fails.
    pub fn sql_dialect(&self, metadata: &SqlFunMetadata) -> Result<SqlDialect, SqlFunArgsError> {
        self.metadata_args.sql_dialect(metadata)
    }

    /// get CTE catalog directory
    ///
    /// # Errors
    ///
    /// Propagates [`SqlFunArgsError`] from metadata lookups.
    pub fn cte_catalog_dir(
        &self,
        metadata: &SqlFunMetadata,
    ) -> Result<Option<PathBuf>, SqlFunArgsError> {
        self.metadata_args.cte_catalog_dir(metadata)
    }

    /// get command
    #[must_use]
    pub fn command(&self) -> &CliSubCommand {
        &self.command
    }

    /// get `PostgreSQL` version
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the version cannot be determined.
    pub fn postgres_version(
        &self,
        metadata: &SqlFunMetadata,
    ) -> Result<EngineVersion, SqlFunArgsError> {
        self.postgres_args.postgres_version(metadata)
    }

    /// get `PostgreSQL` search path
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when metadata retrieval fails.
    pub fn postgres_search_path(
        &self,
        metadata: &SqlFunMetadata,
    ) -> Result<Vec<String>, SqlFunArgsError> {
        self.postgres_args.postgres_search_path(metadata)
    }

    /// get postgres extensions
    ///
    /// # Errors
    ///
    /// Propagates [`SqlFunArgsError`] if metadata lookup fails or the extension
    /// configuration is invalid.
    pub fn postgres_extensions(
        &self,
        metadata: &SqlFunMetadata,
    ) -> Result<PostgresExtensionsCollection, SqlFunArgsError> {
        self.postgres_args.postgres_extensions(metadata)
    }

    /// get builtin schema directory path
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] if the builtin directory cannot be resolved.
    pub fn builtin_info_dir(&self, metadata: &SqlFunMetadata) -> Result<PathBuf, SqlFunArgsError> {
        if let Some(arg) = self.postgres_args.builtin_schema_dir() {
            Ok(arg.clone())
        } else {
            let home = self.sql_fun_home()?;
            let postgres_version = self.postgres_version(metadata)?;
            Ok(postgres_version.definition_base_path(home))
        }
    }

    /// set directory path of extensions
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the path components cannot be derived.
    pub fn postgres_extensions_dir(
        &self,
        metadata: &SqlFunMetadata,
    ) -> Result<PathBuf, SqlFunArgsError> {
        if let Some(ext_dir) = self.postgres_args.postgres_extensions_dir() {
            Ok(ext_dir.clone())
        } else {
            let home = self.sql_fun_home()?;
            let dialect = self.sql_dialect(metadata)?;
            let ver = self.postgres_version(metadata)?;
            Ok(home
                .join(dialect.to_string())
                .join(ver.to_string())
                .join("extensions"))
        }
    }

    const SQL_FUN_OUTPUT_FORMAT: &str = "SQL_FUN_OUTPUT_FORMAT";
    const SQL_FUN_NO_BUILTIN: &str = "SQL_FUN_NO_BUILTIN";

    /// get environemnts for child process
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] if metadata fails to load or env generation
    /// encounters an error.
    pub fn environments(&self) -> Result<HashMap<String, String>, SqlFunArgsError> {
        let mut result = HashMap::new();
        let metadata_file = self.metadata_file()?;
        let metadata = SqlFunMetadata::load_from(&metadata_file)?;

        self.metadata_args.get_envs(&mut result, &metadata)?;
        self.postgres_args.get_envs(&mut result, self, &metadata)?;
        self.log_args.get_envs(&mut result);

        result.insert(
            Self::SQL_FUN_OUTPUT_FORMAT.to_string(),
            self.output_format.clone(),
        );

        Ok(result)
    }
}

/// deamon control args
#[derive(clap::Parser, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DaemonControlArgs {
    /// daemon extension name
    daemon_name: String,
    /// daemon arguments
    daemon_args: Vec<String>,
}

impl SqlFunArgs {
    /// Initialize and parse arguments
    #[must_use]
    pub fn parse_args() -> Self {
        let mut args = SqlFunArgs::parse();
        if args.verbose {
            args.log_args.set_log_level("debug");
        }
        args
    }
}

#[cfg(test)]
mod tests {

    use clap::Parser;
    use testresult::TestResult;

    use crate::SqlFunArgs;

    #[test]
    pub fn test_schema_file() -> TestResult {
        let args = SqlFunArgs::try_parse_from(vec![
            "sqlfun",
            "--sql-fun-home",
            "../sql-fun-home",
            "subcmd",
        ])?;
        let metadata_file = args.metadata_file()?;
        let expect_file = metadata_file.with_file_name("sqlfun.develop.sql");
        let mut created = false;
        if !expect_file.exists() {
            std::fs::write(&expect_file, "-- dummy sql file")?;
            created = true;
        }
        let schema_file = args.schema_file(&metadata_file)?;
        if created {
            std::fs::remove_file(&expect_file)?;
        }
        assert_eq!(schema_file, expect_file);
        Ok(())
    }
}
