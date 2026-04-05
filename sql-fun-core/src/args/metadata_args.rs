use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{
    SqlDialect, SqlFunArgsBuilder, SqlFunArgsBuilderError, SqlFunArgsError, SqlFunMetadata,
};

fn path_owner<P: AsRef<Path>>(path: P) -> Result<u32, std::io::Error> {
    let meta = std::fs::metadata(path)?;
    Ok(meta.uid())
}

#[derive(
    clap::Args, Debug, serde::Serialize, serde::Deserialize, Clone, derive_builder::Builder,
)]
#[builder(build_fn(error = "SqlFunArgsBuilderError"))]
pub struct MetadataArgs {
    /// `home_dir`
    #[clap(long, env = Self::SQL_FUN_HOME)]
    #[builder(setter(into, strip_option), default)]
    sql_fun_home: Option<PathBuf>,

    /// Metadata file format (TOML)
    #[clap(long, env = Self::SQL_FUN_METADATA_FORMAT, default_value = "TOML")]
    #[builder(setter(into), default = "Self::sql_fun_metadata_format_default()")]
    metadata_format: String,

    /// Path to metadata file (Cargo.toml)
    #[clap(long, env = Self::SQL_FUN_METADATA_FILE)]
    #[builder(setter(into, strip_option), default)]
    metadata_file: Option<String>,

    /// Schema selector
    #[clap(long, env = Self::SQL_FUN_SCHEMA, default_value = "develop")]
    #[builder(setter(into), default = "Self::sql_fun_metadata_schema_default()")]
    schema: String,

    /// Schema dump file (overrides --schema)
    #[clap(long, env = Self::SQL_FUN_SCHEMA_FILE )]
    #[builder(setter(into, strip_option), default)]
    schema_file: Option<PathBuf>,

    /// SQL dialect (e.g., postgresql)
    #[clap(long, env = Self::SQL_FUN_DIALECT)]
    #[builder(setter(into, strip_option), default)]
    sql_dialect: Option<String>,

    /// Directory for CTE catalog
    #[clap(long, env = Self::SQL_FUN_CTE_CATALOG_DIR)]
    #[builder(setter(into, strip_option), default)]
    cte_catalog_dir: Option<PathBuf>,
}

impl MetadataArgsBuilder {
    fn sql_fun_metadata_format_default() -> String {
        String::from(MetadataArgs::SQL_FUN_METADATA_FORMAT_DEFAULT)
    }
}

impl MetadataArgsBuilder {
    fn sql_fun_metadata_schema_default() -> String {
        String::from(MetadataArgs::SQL_FUN_METADATA_SCHEMA_DEFAULT)
    }
}

impl SqlFunArgsBuilder {
    /// `home_dir`
    pub fn sql_fun_home<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<PathBuf>,
    {
        self.metadata_args.sql_fun_home(value);
        self
    }
}

impl SqlFunArgsBuilder {
    /// Metadata file format (TOML)
    pub fn metadata_format<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<String>,
    {
        self.metadata_args.metadata_format(value);
        self
    }
}

impl SqlFunArgsBuilder {
    /// Path to metadata file (Cargo.toml)
    pub fn metadata_file<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<String>,
    {
        self.metadata_args.metadata_file(value);
        self
    }
}

impl SqlFunArgsBuilder {
    /// Schema selector
    pub fn schema<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<String>,
    {
        self.metadata_args.schema(value);
        self
    }
}

impl SqlFunArgsBuilder {
    /// Schema dump file (overrides --schema)
    pub fn schema_file<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<PathBuf>,
    {
        self.metadata_args.schema_file(value);
        self
    }
}

impl SqlFunArgsBuilder {
    /// SQL Dialect
    pub fn sql_dialect<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<SqlDialect>,
    {
        let dialect = value.into();
        self.metadata_args.sql_dialect(dialect.to_string());
        self
    }
}

impl SqlFunArgsBuilder {
    /// Directory for CTE catalog
    pub fn cte_catalog_dir<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<PathBuf>,
    {
        self.metadata_args.cte_catalog_dir(value);
        self
    }
}

impl MetadataArgs {
    const SQL_FUN_HOME: &str = "SQL_FUN_HOME";
    const SQL_FUN_METADATA_FORMAT: &str = "SQL_FUN_METADATA_FORMAT";
    const SQL_FUN_METADATA_FILE: &str = "SQL_FUN_METADATA_FILE";
    const SQL_FUN_SCHEMA: &str = "SQL_FUN_SCHEMA";
    const SQL_FUN_SCHEMA_FILE: &str = "SQL_FUN_SCHEMA_FILE";
    const SQL_FUN_DIALECT: &str = "SQL_FUN_DIALECT";
    const SQL_FUN_CTE_CATALOG_DIR: &str = "SQL_FUN_CTE_CATALOG_DIR";

    const SQL_FUN_METADATA_FORMAT_DEFAULT: &str = "TOML";
    const SQL_FUN_METADATA_SCHEMA_DEFAULT: &str = "develop";
}

impl MetadataArgs {
    pub(crate) fn apply_envs(
        builder: &mut SqlFunArgsBuilder,
        envs: &HashMap<String, String>,
    ) -> Result<(), SqlFunArgsError> {
        if let Some(value) = envs.get(Self::SQL_FUN_HOME) {
            builder.sql_fun_home(value);
        }
        if let Some(value) = envs.get(Self::SQL_FUN_METADATA_FORMAT) {
            builder.metadata_format(value);
        }
        if let Some(value) = envs.get(Self::SQL_FUN_METADATA_FILE) {
            builder.metadata_file(value);
        }
        if let Some(value) = envs.get(Self::SQL_FUN_SCHEMA) {
            builder.schema(value);
        }
        if let Some(value) = envs.get(Self::SQL_FUN_SCHEMA_FILE) {
            builder.schema_file(PathBuf::from(value));
        }
        if let Some(value) = envs.get(Self::SQL_FUN_DIALECT) {
            let Ok(dialect) = SqlDialect::from_str(value) else {
                return SqlFunArgsError::not_suppoted(value, "SQL Dialect");
            };
            builder.sql_dialect(dialect);
        }
        if let Some(value) = envs.get(Self::SQL_FUN_CTE_CATALOG_DIR) {
            builder.cte_catalog_dir(PathBuf::from(value));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_apply_envs {
    use testresult::TestResult;

    use super::{MetadataArgs, SqlFunArgsBuilder};
    use crate::{
        SqlDialect, SqlFunArgsError,
        test_helpers::{
            TestMetadataEnvs, TestProjectDir, parse_postgres_metadata,
            write_metadata_project_with_cte,
        },
    };

    #[test]
    fn applies_metadata_related_environment_values() -> TestResult {
        let home_dir = TestProjectDir::new()?;
        let project_dir = TestProjectDir::new()?;
        let fixture = write_metadata_project_with_cte(&project_dir, "tmp", "qa", "cte")?;

        let envs = TestMetadataEnvs::new()
            .with_sql_fun_home(home_dir.path())
            .with_metadata_project_with_cte(&fixture, "qa")
            .with_sql_dialect("postgresql")
            .into_map();

        let mut builder = SqlFunArgsBuilder::default();
        MetadataArgs::apply_envs(&mut builder, &envs)?;
        let args = builder.build()?;

        let metadata = parse_postgres_metadata(&fixture.metadata_file)?;

        assert_eq!(args.sql_fun_home()?, home_dir.path().canonicalize()?);
        assert_eq!(args.metadata_format(), "TOML");
        assert_eq!(
            args.metadata_file_raw()?,
            fixture.metadata_file.to_string_lossy().as_ref()
        );
        assert_eq!(
            args.schema_file(&fixture.metadata_file)?,
            fixture.schema_file
        );
        assert_eq!(args.sql_dialect(&metadata)?, SqlDialect::PostgreSQL);
        assert_eq!(
            args.cte_catalog_dir(&metadata)?,
            Some(fixture.cte_catalog_dir.clone())
        );

        Ok(())
    }

    #[test]
    fn rejects_unsupported_sql_dialect_env_value() {
        let envs = TestMetadataEnvs::new()
            .with_sql_dialect("sqlite")
            .into_map();

        let err = MetadataArgs::apply_envs(&mut SqlFunArgsBuilder::default(), &envs)
            .expect_err("expected invalid dialect to fail");

        assert!(matches!(
            err,
            SqlFunArgsError::NotSupported(category, value)
            if category == "SQL Dialect" && value == "sqlite"
        ));
    }
}

impl MetadataArgs {
    /// get home dir path
    ///
    /// If `--sql-fun-home` or `SQL_FUN_HOME` env present : configured value
    /// Else returns `${HOME}/.sql-fun`
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when the directory cannot be determined or does not exist.
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
}

impl MetadataArgs {
    /// gets metadata format value in args
    pub fn metadata_format(&self) -> &str {
        &self.metadata_format
    }
}

impl MetadataArgs {
    /// return raw value for `--metadata-file`
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError::NotSupported`] when the format is not `TOML`.
    pub fn metadata_file_raw(&self) -> Result<&str, SqlFunArgsError> {
        if let Some(metadata_file) = self.metadata_file.as_deref() {
            Ok(metadata_file)
        } else if self.metadata_format() == "TOML" {
            Ok("Cargo.toml")
        } else {
            Err(SqlFunArgsError::NotSupported(
                String::from("--metadata-format"),
                String::from(self.metadata_format()),
            ))
        }
    }
}

#[cfg(test)]
mod test_metadata_file_raw {
    use super::MetadataArgsBuilder;
    use crate::SqlFunArgsError;

    #[test]
    fn returns_default_toml_metadata_file_name() {
        let args = MetadataArgsBuilder::default()
            .build()
            .expect("default metadata args");

        assert_eq!(
            args.metadata_file_raw().expect("default TOML path"),
            "Cargo.toml"
        );
    }

    #[test]
    fn rejects_default_metadata_file_for_unsupported_format() {
        let args = MetadataArgsBuilder::default()
            .metadata_format("JSON")
            .build()
            .expect("metadata args with JSON format");

        let err = args
            .metadata_file_raw()
            .expect_err("expected unsupported format to fail");
        assert!(matches!(
            err,
            SqlFunArgsError::NotSupported(category, value)
            if category == "--metadata-format" && value == "JSON"
        ));
    }
}

impl MetadataArgs {
    /// search metadata file with same-owner policy
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError::FilePathNotFound`] if no matching file is located.
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
            log::trace!("searching {} with owner {owner}", raw_arg.display());
            for ancestor in current_dir.ancestors() {
                let dir_owner = path_owner(ancestor)?;
                if owner != dir_owner {
                    log::error!("owner not match for {}", ancestor.display());
                    break;
                }
                let mut file = ancestor.to_path_buf();
                file.push(&raw_arg);
                if file.exists() {
                    let file_owner = path_owner(&file)?;
                    if owner == file_owner {
                        return Ok(file);
                    }
                    log::error!("owner not match for {}", file.display());
                    break;
                }
                log::trace!("{} not exist", file.display());
            }
            SqlFunArgsError::file_path_not_found(raw_arg)?
        }
    }
}

#[cfg(test)]
mod test_metadata_file {
    use testresult::TestResult;

    use super::MetadataArgsBuilder;
    use crate::test_helpers::{TestProjectDir, write_test_manifest};

    #[test]
    fn returns_existing_absolute_metadata_file_path() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let metadata_file = write_test_manifest(&project_dir, "fixture")?;

        let args = MetadataArgsBuilder::default()
            .metadata_file(metadata_file.to_string_lossy().into_owned())
            .build()?;

        assert_eq!(args.metadata_file()?, metadata_file);

        Ok(())
    }
}

impl MetadataArgs {
    /// get schema file
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError::FilePathNotFound`] if the schema file does not exist.
    pub fn schema_file(&self, metadata_file: &Path) -> Result<PathBuf, SqlFunArgsError> {
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
                    "unexpected metadata_file.parent() is none: {}",
                    metadata_file.display()
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
}

#[cfg(test)]
mod test_schema_file {
    use testresult::TestResult;

    use super::MetadataArgsBuilder;
    use crate::test_helpers::{
        TestMetadataProject, TestProjectDir, write_metadata_project, write_sql_fixture,
        write_test_manifest,
    };

    #[test]
    fn returns_explicit_schema_file_when_present() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let metadata_file = write_test_manifest(&project_dir, "fixture")?;
        let explicit_schema_file = write_sql_fixture(&project_dir, "custom.sql")?;

        let args = MetadataArgsBuilder::default()
            .metadata_file(metadata_file.to_string_lossy().into_owned())
            .schema_file(explicit_schema_file.clone())
            .build()?;

        assert_eq!(args.schema_file(&metadata_file)?, explicit_schema_file);

        Ok(())
    }

    #[test]
    fn derives_schema_file_from_metadata_file_parent() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let TestMetadataProject {
            metadata_file,
            schema_file,
        } = write_metadata_project(&project_dir, "fixture", "qa")?;

        let args = MetadataArgsBuilder::default()
            .metadata_file(metadata_file.to_string_lossy().into_owned())
            .schema("qa")
            .build()?;

        assert_eq!(args.schema_file(&metadata_file)?, schema_file);

        Ok(())
    }
}

impl MetadataArgs {
    /// get SQL dialect
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError::NotSupported`] for unknown dialect names.
    pub fn sql_dialect(&self, metadata: &SqlFunMetadata) -> Result<SqlDialect, SqlFunArgsError> {
        let Some(dialect) = &self.sql_dialect else {
            return Ok(metadata.sql_dialect());
        };
        let Ok(dialect) = SqlDialect::from_str(dialect) else {
            SqlFunArgsError::not_suppoted(dialect, "SQL Dialect")?
        };
        Ok(dialect)
    }
}

#[cfg(test)]
mod test_sql_dialect {
    use super::MetadataArgsBuilder;
    use crate::{SqlDialect, SqlFunArgsError, test_helpers::parse_postgres_metadata};

    #[test]
    fn falls_back_to_metadata_sql_dialect() {
        let metadata = parse_postgres_metadata("Cargo.toml").expect("metadata fixture");
        let args = MetadataArgsBuilder::default()
            .build()
            .expect("default metadata args");

        assert_eq!(
            args.sql_dialect(&metadata).expect("metadata dialect"),
            SqlDialect::PostgreSQL
        );
    }

    #[test]
    fn rejects_unsupported_explicit_sql_dialect() {
        let metadata = parse_postgres_metadata("Cargo.toml").expect("metadata fixture");
        let args = MetadataArgsBuilder::default()
            .sql_dialect("sqlite")
            .build()
            .expect("metadata args with explicit dialect");

        let err = args
            .sql_dialect(&metadata)
            .expect_err("expected invalid explicit dialect to fail");

        assert!(matches!(
            err,
            SqlFunArgsError::NotSupported(category, value)
            if category == "SQL Dialect" && value == "sqlite"
        ));
    }
}

impl MetadataArgs {
    /// get CTE catalog directory
    ///
    /// # Errors
    ///
    /// Propagates [`SqlFunArgsError`] from metadata lookups.
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
}

#[cfg(test)]
mod test_cte_catalog_dir {
    use testresult::TestResult;

    use super::MetadataArgsBuilder;
    use crate::test_helpers::{TestProjectDir, parse_postgres_metadata_with_cte_catalog};

    #[test]
    fn prefers_argument_cte_catalog_dir_over_metadata_value() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let arg_cte_catalog = project_dir.create_dir("arg-cte")?;
        let metadata_cte_catalog = project_dir.create_dir("metadata-cte")?;

        let metadata = parse_postgres_metadata_with_cte_catalog(
            project_dir.path().join("Cargo.toml"),
            &metadata_cte_catalog,
        )?;
        let args = MetadataArgsBuilder::default()
            .cte_catalog_dir(arg_cte_catalog.clone())
            .build()?;

        assert_eq!(args.cte_catalog_dir(&metadata)?, Some(arg_cte_catalog));

        Ok(())
    }

    #[test]
    fn falls_back_to_metadata_cte_catalog_dir() -> TestResult {
        let project_dir = TestProjectDir::new()?;
        let metadata_cte_catalog = project_dir.create_dir("metadata-cte")?;

        let metadata = parse_postgres_metadata_with_cte_catalog(
            project_dir.path().join("Cargo.toml"),
            &metadata_cte_catalog,
        )?;
        let args = MetadataArgsBuilder::default().build()?;

        assert_eq!(args.cte_catalog_dir(&metadata)?, Some(metadata_cte_catalog));

        Ok(())
    }
}

impl MetadataArgs {
    /// Populate env map with derived values.
    ///
    /// # Errors
    ///
    /// Returns [`SqlFunArgsError`] when metadata resolution fails.
    pub fn get_envs(
        &self,
        envs: &mut HashMap<String, String>,
        metadata: &SqlFunMetadata,
    ) -> Result<(), SqlFunArgsError> {
        let metadata_file = self.metadata_file()?;

        envs.insert(
            Self::SQL_FUN_HOME.to_string(),
            self.sql_fun_home()?.to_string_lossy().to_string(),
        );
        envs.insert(
            Self::SQL_FUN_METADATA_FORMAT.to_string(),
            self.metadata_format().to_string(),
        );
        envs.insert(
            Self::SQL_FUN_METADATA_FILE.to_string(),
            metadata_file.to_string_lossy().to_string(),
        );
        envs.insert(Self::SQL_FUN_SCHEMA.to_string(), self.schema.clone());
        envs.insert(
            Self::SQL_FUN_SCHEMA_FILE.to_string(),
            self.schema_file(&metadata_file)?
                .to_string_lossy()
                .to_string(),
        );
        envs.insert(
            Self::SQL_FUN_DIALECT.to_string(),
            self.sql_dialect(metadata)?.to_string(),
        );
        if let Some(cte_catalog_dir) = self.cte_catalog_dir(metadata)? {
            envs.insert(
                Self::SQL_FUN_CTE_CATALOG_DIR.to_string(),
                cte_catalog_dir.to_string_lossy().to_string(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_get_envs {
    use std::collections::HashMap;

    use testresult::TestResult;

    use super::MetadataArgsBuilder;
    use crate::test_helpers::{
        TestMetadataEnvs, TestProjectDir, parse_postgres_metadata_with_cte_catalog,
        write_metadata_project_with_cte,
    };

    #[test]
    fn populates_env_map_with_resolved_metadata_values() -> TestResult {
        let home_dir = TestProjectDir::new()?;
        let project_dir = TestProjectDir::new()?;
        let fixture = write_metadata_project_with_cte(&project_dir, "fixture", "qa", "cte")?;

        let metadata = parse_postgres_metadata_with_cte_catalog(
            &fixture.metadata_file,
            &fixture.cte_catalog_dir,
        )?;
        let args = MetadataArgsBuilder::default()
            .sql_fun_home(home_dir.path())
            .metadata_file(fixture.metadata_file.to_string_lossy().into_owned())
            .schema("qa")
            .build()?;
        let mut envs = HashMap::new();

        args.get_envs(&mut envs, &metadata)?;

        let expected_envs = TestMetadataEnvs::new()
            .with_sql_fun_home(&home_dir.path().canonicalize()?)
            .with_metadata_project_with_cte(&fixture, "qa")
            .with_sql_dialect("postgresql")
            .into_map();

        assert_eq!(envs, expected_envs);

        Ok(())
    }
}
