mod error;
mod home_path;
mod impl_metadata;
mod impl_sql_fun_home;
mod init_args;
mod log_args;
mod metadata_args;
mod postgres;

use std::{
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
};

use clap::Parser;

pub use self::{
    error::SqlFunArgsError,
    home_path::resolve_sql_fun_home_path,
    init_args::InitializeArgs,
    log_args::{LoggingArgs, LoggingArgsBuilder},
    metadata_args::{MetadataArgs, MetadataArgsBuilder},
    postgres::{PostgresArgs, PostgresArgsBuilder},
};

use crate::{
    HighlighterTheme, PostgresExtensionsCollection, SqlDialect, SqlFunMetadata, TerminalColor,
    metadata::{PostgresEngineVersion, SqlFunMetadataCollection},
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

    /// datasource name
    #[clap(long, env = Self::SQL_FUN_DATASOURCE)]
    #[builder(default)]
    datasource: Option<String>,

    /// proc-macro mode
    #[clap(long, env = Self::SQL_FUN_MODE)]
    #[builder(default)]
    mode: Option<String>,
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
        if let Some(datasource) = envs.get(Self::SQL_FUN_DATASOURCE) {
            builder.datasource(Some(datasource.clone()));
        }

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

impl SqlFunArgs {
    /// Restore explicitly transferred configuration without reading process variables.
    /// Home and metadata must be selected so a receiver cannot discover ambient assets.
    /// Unknown environment keys follow the existing builder's behavior and are ignored.
    ///
    /// # Errors
    /// Rejects malformed settings and omitted or empty home/metadata selections.
    pub fn from_environments(envs: &HashMap<String, String>) -> Result<Self, SqlFunArgsError> {
        let args = Self::builder(envs)?
            .build()
            .map_err(|error| SqlFunArgsError::Unexpected(error.to_string()))?;
        if args
            .initialization_home_candidate()
            .is_none_or(|path| path.as_os_str().is_empty())
        {
            return Err(SqlFunArgsError::Unexpected(
                "explicit SQL_FUN_HOME is required".into(),
            ));
        }
        if !args.metadata_file_is_explicit() || args.metadata_file_raw()?.is_empty() {
            return Err(SqlFunArgsError::Unexpected(
                "explicit SQL_FUN_METADATA_FILE is required".into(),
            ));
        }
        Ok(args)
    }

    /// Load only the datasource explicitly selected by a caller.
    ///
    /// # Errors
    /// Rejects unavailable metadata and missing or malformed selected datasources.
    pub fn selected_metadata(&self, datasource: &str) -> Result<SqlFunMetadata, SqlFunArgsError> {
        let declarations = crate::InitializationDatasources::load_from(&self.metadata_file()?)?;
        Ok(declarations.selected_metadata(datasource)?)
    }
}

#[cfg(test)]
mod test_transferred_configuration {
    use super::SqlFunArgs;
    use crate::test_helpers::{TestMetadataEnvs, TestProjectDir, write_postgres_metadata_project};

    #[test]
    fn restores_core_serialized_configuration_and_overrides_metadata() {
        let project = TestProjectDir::new().unwrap();
        let fixture = write_postgres_metadata_project(&project);
        let original = SqlFunArgs::builder(
            &TestMetadataEnvs::new()
                .with_sql_fun_home(project.path())
                .with_metadata_project(&fixture, "qa")
                .into_map(),
        )
        .unwrap()
        .datasource(Some("default".to_owned()))
        .postgres_version("17".parse::<crate::PostgresEngineVersion>().unwrap())
        .postgres_search_path(vec!["chosen".to_owned()])
        .builtin_schema_dir(project.path().join("selected-builtins"))
        .postgres_extensions_dir(project.path().join("selected-extensions"))
        .build()
        .unwrap();
        let restored = SqlFunArgs::from_environments(&original.environments().unwrap()).unwrap();
        assert_eq!(restored.datasource(), Some("default"));
        let metadata = restored.selected_metadata("default").unwrap();
        assert_eq!(
            restored.postgres_search_path(&metadata).unwrap(),
            vec!["chosen"]
        );
        assert_eq!(
            restored.postgres_version(&metadata).unwrap().to_string(),
            "17"
        );
        assert_eq!(
            restored.builtin_info_dir(&metadata).unwrap(),
            original.builtin_info_dir(&metadata).unwrap()
        );
        assert_eq!(
            restored.postgres_extensions_dir(&metadata).unwrap(),
            original.postgres_extensions_dir(&metadata).unwrap()
        );
        assert_eq!(restored.sql_fun_home().unwrap(), project.path());
        assert!(restored.selected_metadata("missing").is_err());
    }

    #[rstest::rstest]
    #[case::omitted_home(false)]
    #[case::omitted_metadata(true)]
    fn omitted_assets_do_not_use_process_configuration(#[case] home: bool) {
        let envs = if home {
            TestMetadataEnvs::new().with_sql_fun_home(std::path::Path::new("/chosen-home"))
        } else {
            TestMetadataEnvs::new().with_metadata_file(std::path::Path::new("/chosen-metadata"))
        }
        .into_map();
        assert!(SqlFunArgs::from_environments(&envs).is_err());
    }
}

fn parse_terminal_color(value: &str) -> Result<TerminalColor, SqlFunArgsError> {
    match value.to_lowercase().as_str() {
        "auto" => Ok(TerminalColor::Auto),
        "ansi" => Ok(TerminalColor::Ansi),
        "never" => Ok(TerminalColor::Never),
        _ => SqlFunArgsError::not_supported(value, "Terminal Color"),
    }
}

fn parse_bool_env(value: &str) -> Result<bool, SqlFunArgsError> {
    match value.to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => SqlFunArgsError::not_supported(value, "Boolean"),
    }
}

impl SqlFunArgs {
    /// get current schema
    #[must_use]
    pub fn schema(&self) -> &str {
        self.metadata_args.schema()
    }

    /// return true when check mode
    #[must_use]
    pub fn is_check_mode(&self) -> bool {
        let Some(mode) = &self.mode else {
            return false;
        };
        mode == "CHECK"
    }

    /// load metadata
    ///
    /// # Errors
    ///
    /// [`SqlFunArgsError`] when metadata configuration or IO error
    ///
    #[tracing::instrument(skip(self))]
    pub fn load_metadata(&self) -> Result<SqlFunMetadataCollection, SqlFunArgsError> {
        let metadata_file = self.metadata_file()?;
        Ok(SqlFunMetadataCollection::load_from(metadata_file)?)
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

    /// `PostgreSQL` version explicitly selected by CLI or environment, without project metadata.
    ///
    /// # Errors
    /// Returns an error if the configured version cannot be parsed.
    pub fn postgres_version_override(
        &self,
    ) -> Result<Option<PostgresEngineVersion>, SqlFunArgsError> {
        self.postgres_args.version_override()
    }

    /// Whether metadata was explicitly configured rather than discovered by default.
    #[must_use]
    pub const fn metadata_file_is_explicit(&self) -> bool {
        self.metadata_args.has_explicit_file()
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

    /// Resolve the common home path before initialization creates it.
    ///
    /// Uses the same CLI/environment/default precedence as analysis.
    ///
    /// # Errors
    /// Returns an error when the current directory or user home cannot be resolved.
    pub fn initialization_home(&self) -> Result<PathBuf, SqlFunArgsError> {
        self.metadata_args.initialization_home()
    }

    /// Return the configured home path without selecting or resolving it.
    #[must_use]
    pub fn initialization_home_candidate(&self) -> Option<&Path> {
        self.metadata_args.initialization_home_candidate()
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
    const SQL_FUN_DATASOURCE: &str = "SQL_FUN_DATASOURCE";
    const SQL_FUN_MODE: &str = "SQL_FUN_MODE";

    /// selected datasource name
    #[must_use]
    pub fn datasource(&self) -> Option<&str> {
        self.datasource.as_ref().map(std::convert::AsRef::as_ref)
    }

    /// get environments for child process
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] if metadata fails to load or env generation
    /// encounters an error.
    pub fn environments(&self) -> Result<HashMap<String, String>, SqlFunArgsError> {
        let mut result = HashMap::new();
        let metadata_file = self.metadata_file()?;
        let metadatas = SqlFunMetadataCollection::load_from(&metadata_file)?;
        if let Some(datasource) = &self.datasource {
            let Some(metadata) = metadatas.get(datasource) else {
                return SqlFunArgsError::datasource_not_found(datasource);
            };
            self.metadata_args.get_envs(&mut result, metadata)?;
            self.postgres_args.get_envs(&mut result, self, metadata)?;

            result.insert(Self::SQL_FUN_DATASOURCE.to_string(), datasource.clone());
        }

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
    /// Returns [`SqlFunArgsError`] if failed load theme.
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

    /// get project dir
    ///
    /// # Errors
    ///
    /// Returns an error if the metadata file cannot be located.
    pub fn project_dir(&self) -> Result<PathBuf, SqlFunArgsError> {
        let mut project_dir = self.metadata_file()?;
        project_dir.pop();
        Ok(project_dir)
    }

    fn with_file_name<P: AsRef<Path>>(mut dir_path: PathBuf, file_name: P) -> PathBuf {
        dir_path.push(file_name);
        dir_path
    }

    /// get lockfile path
    ///
    /// # Errors
    ///
    /// Returns an error if the metadata file cannot be located.
    pub fn lockfile_path(&self, datasource_name: &str) -> Result<PathBuf, SqlFunArgsError> {
        let schema = self.schema();
        Ok(Self::with_file_name(
            self.project_dir()?,
            format!("{datasource_name}.{schema}.lock"),
        ))
    }

    /// get schema dump path
    ///
    /// # Errors
    ///
    /// Returns an error if the metadata file cannot be located.
    pub fn schema_dump_path(&self, datasource_name: &str) -> Result<PathBuf, SqlFunArgsError> {
        let schema = self.schema();
        Ok(Self::with_file_name(
            self.project_dir()?,
            format!("{datasource_name}.{schema}.sql"),
        ))
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
    #[tracing::instrument]
    pub fn try_from_env() -> Result<Self, SqlFunArgsError> {
        let mut args = Self::try_parse_from(Vec::<String>::default())?;
        tracing::info!("argument loaded");
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
