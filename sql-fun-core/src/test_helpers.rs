use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{MetadataError, SqlFunMetadata};

/// Temporary project directory used by unit tests.
pub struct TestProjectDir(tempfile::TempDir);

/// Test fixture containing a manifest and matching schema dump.
pub struct TestMetadataProject {
    /// Path to the generated manifest fixture.
    pub metadata_file: PathBuf,
    /// Path to the generated schema dump fixture.
    pub schema_file: PathBuf,
}

/// Test fixture containing a manifest, schema dump, and CTE catalog directory.
pub struct TestMetadataProjectWithCte {
    /// Path to the generated manifest fixture.
    pub metadata_file: PathBuf,
    /// Path to the generated schema dump fixture.
    pub schema_file: PathBuf,
    /// Path to the generated CTE catalog directory fixture.
    pub cte_catalog_dir: PathBuf,
}

/// Builder for environment collections used by metadata argument tests.
#[derive(Default)]
pub struct TestMetadataEnvs {
    entries: HashMap<String, String>,
}

impl TestProjectDir {
    /// Create a temporary project directory.
    pub fn new() -> Result<Self, std::io::Error> {
        Ok(Self(tempfile::tempdir()?))
    }

    /// Return the root path of the temporary project.
    pub fn path(&self) -> &Path {
        self.0.path()
    }

    /// Create a directory relative to the project root.
    pub fn create_dir(&self, relative: &str) -> Result<PathBuf, std::io::Error> {
        let path = self.path().join(relative);
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }

    /// Write a file relative to the project root, creating parent directories as needed.
    pub fn write_file(&self, relative: &str, content: &str) -> Result<PathBuf, std::io::Error> {
        let path = self.path().join(relative);
        let Some(parent) = path.parent() else {
            return Err(std::io::Error::other("relative path has no parent"));
        };
        std::fs::create_dir_all(parent)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }
}

impl TestMetadataEnvs {
    /// Create an empty environment collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn with_entry(mut self, key: &str, value: impl Into<String>) -> Self {
        self.entries.insert(key.to_string(), value.into());
        self
    }

    fn with_path(self, key: &str, path: &Path) -> Self {
        self.with_entry(key, path.to_string_lossy().into_owned())
    }

    /// Add `SQL_FUN_HOME`.
    #[must_use]
    pub fn with_sql_fun_home(self, path: &Path) -> Self {
        self.with_path("SQL_FUN_HOME", path)
    }

    /// Add `SQL_FUN_METADATA_FORMAT`.
    #[must_use]
    pub fn with_metadata_format(self, format: &str) -> Self {
        self.with_entry("SQL_FUN_METADATA_FORMAT", format)
    }

    /// Add `SQL_FUN_METADATA_FILE`.
    #[must_use]
    pub fn with_metadata_file(self, path: &Path) -> Self {
        self.with_path("SQL_FUN_METADATA_FILE", path)
    }

    /// Add `SQL_FUN_SCHEMA`.
    #[must_use]
    pub fn with_schema(self, schema: &str) -> Self {
        self.with_entry("SQL_FUN_SCHEMA", schema)
    }

    /// Add `SQL_FUN_SCHEMA_FILE`.
    #[must_use]
    pub fn with_schema_file(self, path: &Path) -> Self {
        self.with_path("SQL_FUN_SCHEMA_FILE", path)
    }

    /// Add `SQL_FUN_DIALECT`.
    #[must_use]
    pub fn with_sql_dialect(self, dialect: &str) -> Self {
        self.with_entry("SQL_FUN_DIALECT", dialect)
    }

    /// Add `SQL_FUN_CTE_CATALOG_DIR`.
    #[must_use]
    pub fn with_cte_catalog_dir(self, path: &Path) -> Self {
        self.with_path("SQL_FUN_CTE_CATALOG_DIR", path)
    }

    /// Add the standard metadata env entries for a manifest and schema dump fixture.
    #[must_use]
    pub fn with_metadata_project(self, fixture: &TestMetadataProject, schema: &str) -> Self {
        self.with_metadata_format("TOML")
            .with_metadata_file(&fixture.metadata_file)
            .with_schema(schema)
            .with_schema_file(&fixture.schema_file)
    }

    /// Add the standard metadata env entries for a manifest, schema dump, and CTE fixture.
    #[must_use]
    pub fn with_metadata_project_with_cte(
        self,
        fixture: &TestMetadataProjectWithCte,
        schema: &str,
    ) -> Self {
        self.with_metadata_format("TOML")
            .with_metadata_file(&fixture.metadata_file)
            .with_schema(schema)
            .with_schema_file(&fixture.schema_file)
            .with_cte_catalog_dir(&fixture.cte_catalog_dir)
    }

    /// Convert the collection into a map.
    #[must_use]
    pub fn into_map(self) -> HashMap<String, String> {
        self.entries
    }
}

/// Parse `SqlFunMetadata` from inline Cargo-style TOML used by unit tests.
pub fn parse_metadata<P: AsRef<Path> + std::fmt::Debug>(
    source: &str,
    path_hint: P,
) -> Result<SqlFunMetadata, MetadataError> {
    SqlFunMetadata::from_str(source, path_hint)
}

/// Parse PostgreSQL metadata without a CTE catalog.
pub fn parse_postgres_metadata<P: AsRef<Path> + std::fmt::Debug>(
    path_hint: P,
) -> Result<SqlFunMetadata, MetadataError> {
    parse_metadata(
        r#"
[package]
name = "fixture"
version = "0.1.0"

[package.metadata.database]
engine = "PostgreSQL"
dialect = "postgres"
"#,
        path_hint,
    )
}

/// Parse PostgreSQL metadata with a CTE catalog path.
pub fn parse_postgres_metadata_with_cte_catalog<P, Q>(
    path_hint: P,
    cte_catalog: Q,
) -> Result<SqlFunMetadata, MetadataError>
where
    P: AsRef<Path> + std::fmt::Debug,
    Q: AsRef<Path>,
{
    parse_metadata(
        &format!(
            r#"
[package]
name = "fixture"
version = "0.1.0"

[package.metadata.database]
engine = "PostgreSQL"
dialect = "postgres"
cte_catalog = "{}"
"#,
            cte_catalog.as_ref().display()
        ),
        path_hint,
    )
}

/// Write a SQL fixture file with a default SQL comment body.
pub fn write_sql_fixture(
    project_dir: &TestProjectDir,
    relative: &str,
) -> Result<PathBuf, std::io::Error> {
    project_dir.write_file(relative, "-- schema")
}

/// Write a schema dump file for the given schema name.
pub fn write_schema_dump(
    project_dir: &TestProjectDir,
    schema: &str,
) -> Result<PathBuf, std::io::Error> {
    write_sql_fixture(project_dir, &format!("sqlfun.{schema}.sql"))
}

/// Write a manifest and matching schema dump for a metadata test fixture.
pub fn write_metadata_project(
    project_dir: &TestProjectDir,
    package_name: &str,
    schema: &str,
) -> Result<TestMetadataProject, std::io::Error> {
    let metadata_file = write_test_manifest(project_dir, package_name)?;
    let schema_file = write_schema_dump(project_dir, schema)?;
    Ok(TestMetadataProject {
        metadata_file,
        schema_file,
    })
}

/// Write a manifest, schema dump, and CTE catalog directory for a metadata test fixture.
pub fn write_metadata_project_with_cte(
    project_dir: &TestProjectDir,
    package_name: &str,
    schema: &str,
    cte_catalog_relative: &str,
) -> Result<TestMetadataProjectWithCte, std::io::Error> {
    let fixture = write_metadata_project(project_dir, package_name, schema)?;
    let cte_catalog_dir = project_dir.create_dir(cte_catalog_relative)?;
    Ok(TestMetadataProjectWithCte {
        metadata_file: fixture.metadata_file,
        schema_file: fixture.schema_file,
        cte_catalog_dir,
    })
}

/// Write a minimal Cargo manifest for tests and return its path.
pub fn write_test_manifest(
    project_dir: &TestProjectDir,
    package_name: &str,
) -> Result<PathBuf, std::io::Error> {
    project_dir.write_file(
        "Cargo.toml",
        &format!("[package]\nname = '{package_name}'\nversion = '0.1.0'\n"),
    )
}
