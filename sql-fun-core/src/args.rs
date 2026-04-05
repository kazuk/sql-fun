mod error;
mod impl_metadata;
mod impl_sql_fun_home;
mod init_args;
mod log_args;
mod metadata_args;
mod postgres;

use std::{collections::HashMap, fmt::Display, path::PathBuf};

use clap::Parser;

pub use self::{
    error::SqlFunArgsError,
    init_args::InitializeArgs,
    log_args::{LoggingArgs, LoggingArgsBuilder},
    metadata_args::{MetadataArgs, MetadataArgsBuilder},
    postgres::{PostgresArgs, PostgresArgsBuilder},
};

use crate::{
    HighlighterTheme, PostgresExtensionsCollection, SqlDialect, SqlFunMetadata, TerminalColor,
    metadata::PostgresEngineVersion,
};

#[derive(Debug)]
pub enum ConfigurationKind {
    EngineVersion,
}

impl Display for ConfigurationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EngineVersion => write!(f, "Database engine version"),
        }
    }
}

/// Common `sql-fun` CLI arguments and environment variables
#[derive(
    clap::Parser, Debug, serde::Serialize, serde::Deserialize, Clone, derive_builder::Builder,
)]
#[clap(name = "sql-fun", version, about, long_about = None)]
pub struct SqlFunArgs {
    /// metadata arguments
    #[command(flatten)]
    #[builder(
        private,
        field(ty = "MetadataArgsBuilder", build = "self.metadata_args.build()?")
    )]
    metadata_args: MetadataArgs,

    /// postgres arguments
    #[command(flatten)]
    #[builder(
        private,
        field(ty = "PostgresArgsBuilder", build = "self.postgres_args.build()?")
    )]
    postgres_args: PostgresArgs,

    #[command(flatten)]
    #[builder(
        private,
        field(ty = "LoggingArgsBuilder", build = "self.log_args.build()?")
    )]
    log_args: LoggingArgs,

    /// Output format (text or json)
    #[clap(long, env = Self::SQL_FUN_OUTPUT_FORMAT, default_value = "text")]
    #[builder(
        setter(into),
        default = "String::from(Self::SQL_FUN_OUTPUT_FORMAT_DEFAULT)"
    )]
    output_format: String,

    /// Terminal Color
    #[clap(long, env = Self::SQL_FUN_TERM_COLOR, default_value = "auto")]
    #[builder(setter(into), default = "Self::SQL_FUN_TERM_COLOR_DEFAULT")]
    term_color: TerminalColor,

    /// Syntax Highlighter Theme
    ///
    /// default values:
    ///
    /// when `--term-color` is `never` : then monochrome plain texts
    /// when `--term-color` is `ansi` : `${SQL_FUN_HOME}/highlighter/default.toml`
    ///
    #[clap(long, env = Self::SQL_FUN_HIGHLIGHTER_THEME)]
    #[builder(setter(into, strip_option), default)]
    highlighter_theme: Option<PathBuf>,

    /// Verbose mode (equivalent to --log-level=debug)
    #[clap(long)]
    #[builder(default)]
    verbose: bool,

    /// Disable loading built-in tables/views
    #[clap(long, env = Self::SQL_FUN_NO_BUILTIN)]
    #[builder(default)]
    no_builtin_tables: bool,
}

impl SqlFunArgsBuilder {
    const SQL_FUN_OUTPUT_FORMAT_DEFAULT: &str = "text";
    const SQL_FUN_TERM_COLOR_DEFAULT: TerminalColor = TerminalColor::Auto;
}

impl SqlFunArgs {
    /// create a builder instance with environments
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when parsing environment values fails.
    pub fn builder(envs: &HashMap<String, String>) -> Result<SqlFunArgsBuilder, SqlFunArgsError> {
        let mut builder = SqlFunArgsBuilder::default();

        MetadataArgs::apply_envs(&mut builder, envs)?;
        PostgresArgs::apply_envs(&mut builder, envs)?;
        LoggingArgs::apply_envs(&mut builder, envs);

        if let Some(value) = envs.get(Self::SQL_FUN_OUTPUT_FORMAT) {
            builder.output_format(value);
        }
        if let Some(value) = envs.get(Self::SQL_FUN_TERM_COLOR) {
            let color = parse_terminal_color(value)?;
            builder.term_color(color);
        }
        if let Some(value) = envs.get(Self::SQL_FUN_HIGHLIGHTER_THEME) {
            builder.highlighter_theme(PathBuf::from(value));
        }
        if let Some(value) = envs.get(Self::SQL_FUN_NO_BUILTIN) {
            builder.no_builtin_tables(parse_bool_env(value)?);
        }

        Ok(builder)
    }
}

fn parse_terminal_color(value: &str) -> Result<TerminalColor, SqlFunArgsError> {
    match value.to_lowercase().as_str() {
        "auto" => Ok(TerminalColor::Auto),
        "ansi" => Ok(TerminalColor::Ansi),
        "never" => Ok(TerminalColor::Never),
        _ => SqlFunArgsError::not_suppoted(value, "Terminal Color"),
    }
}

fn parse_bool_env(value: &str) -> Result<bool, SqlFunArgsError> {
    match value.to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => SqlFunArgsError::not_suppoted(value, "Boolean"),
    }
}

impl SqlFunArgs {
    /// load metadata
    ///
    /// # Errors
    ///
    /// [`SqlFunArgsError`] when metadata configuration or IO error
    ///
    pub fn load_metadata(&self) -> Result<SqlFunMetadata, SqlFunArgsError> {
        let metadata_file = self.metadata_file()?;
        Ok(SqlFunMetadata::load_from(metadata_file)?)
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

    /// get `PostgreSQL` version
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the version cannot be determined.
    pub fn postgres_version(
        &self,
        metadata: &SqlFunMetadata,
    ) -> Result<PostgresEngineVersion, SqlFunArgsError> {
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
    #[must_use]
    pub fn postgres_extensions(&self, metadata: &SqlFunMetadata) -> PostgresExtensionsCollection {
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
    const SQL_FUN_TERM_COLOR: &str = "SQL_FUN_TERM_COLOR";
    const SQL_FUN_HIGHLIGHTER_THEME: &str = "SQL_FUN_HIGHLIGHTER_THEME";

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

    /// get terminal color value
    #[must_use]
    pub fn term_color(&self) -> TerminalColor {
        self.term_color.get_value()
    }

    /// get syntax highlighter theme
    ///
    /// when [`Self::term_color`] is never, returns [`HighlighterTheme::default`]
    /// when [`Self::highlighter_theme`] is None, returns `${SQL_FUN_HOME}/highlighter/default.toml`
    /// when [`Self::highlighter_theme`] is Some and relative path,
    ///     returns ${SQL_FUN_HOME}/highlighter/specified-path
    /// when [`Self::highlighter_theme`] is Some and absolute path,
    ///     returns load from absolute path
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] if faild load theme.
    ///
    pub fn highlighter_theme(&self) -> Result<HighlighterTheme, SqlFunArgsError> {
        let term_color = self.term_color();
        if term_color.is_never() {
            Ok(HighlighterTheme::default())
        } else {
            let theme_path = if let Some(theme_path) = &self.highlighter_theme {
                if theme_path.is_absolute() {
                    theme_path.clone()
                } else {
                    let mut base = self.sql_fun_home()?;
                    base.push("highlighter");
                    base.push(theme_path);
                    base
                }
            } else {
                let mut path = self.sql_fun_home()?;
                path.push("highlighter/default.toml");
                path
            };
            Ok(HighlighterTheme::from_toml(theme_path)?)
        }
    }
}


impl SqlFunArgs {
    /// Initialize and parse arguments
    #[must_use]
    pub fn parse_args() -> Self {
        let mut args = Self::parse();
        if args.verbose {
            args.log_args.set_log_level("debug");
        }
        args
    }

    /// Initialize for `proc_macro`
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] if failed
    pub fn try_from_env() -> Result<Self, SqlFunArgsError> {
        let mut args = Self::try_parse_from(Vec::<String>::default())?;
        if args.verbose {
            args.log_args.set_log_level("debug");
        }
        Ok(args)
    }
}

#[cfg(test)]
mod tests {

    use clap::Parser;
    use testresult::TestResult;

    use crate::SqlFunArgs;

    #[test]
    pub fn test_schema_file() -> TestResult {
        let args = SqlFunArgs::try_parse_from(vec!["sqlfun", "--sql-fun-home", "../sql-fun-home"])?;
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
