use crate::PostgresVersionInventory;

use std::path::PathBuf;

use ::clap::{CommandFactory, ValueHint};

/// Initialize `sql-fun` environments
#[derive(clap::Parser, Debug, Clone)]
pub struct InitializeArgs {
    /// Disable prompts (assume yes)
    ///
    /// Run non-interactively. Assume "yes" to all prompts, including creating
    /// directories. Existing builtin installations are never overwritten.
    ///
    ///
    #[arg(long = "yes", short = 'y')]
    assume_yes: bool,

    /// `sql-fun` Home directory path. (optional)
    ///
    /// Path to the sql-fun home directory.
    ///
    /// This is where catalogs, extensions, and configuration files are stored.
    /// If not specified, defaults to "${HOME}/.sql-fun".
    ///
    /// [default: ${HOME}/.sql-fun]
    ///
    #[arg(long, short = 'H', value_hint = ValueHint::DirPath)]
    sql_fun_home: Option<PathBuf>,

    /// Extract builtins from an existing server (otherwise use Docker).
    /// Uses libpq connection strings; prefer password files to inline credentials.
    #[arg(
        long,
        env = "SQL_FUN_DATABASE_URL",
        hide_env_values = true,
        conflicts_with = "docker"
    )]
    database_url: Option<String>,

    /// Extract each selected version using its registered PostgreSQL Docker image.
    #[arg(long, conflicts_with = "database_url")]
    docker: bool,

    /// Address used to reach published PostgreSQL ports on the Docker host.
    #[arg(long, default_value = "127.0.0.1", conflicts_with = "database_url")]
    docker_host_address: String,

    /// Docker host interface on which PostgreSQL ports are published.
    #[arg(long, default_value = "127.0.0.1", conflicts_with = "database_url")]
    docker_bind_address: String,

    /// Extraction timeout in seconds for each selected version.
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u64).range(1..))]
    timeout_seconds: u64,

    /// Limit initialization to the specified database engine versions.
    ///
    /// Accepts multiple values (repeatable) or comma-separated lists.
    ///
    /// [default: all supported stable versions for Docker; detected version for a server]
    ///
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    engine_versions: Option<Vec<String>>,
}

impl InitializeArgs {
    /// returns `assume_yes` value
    #[must_use]
    pub const fn assume_yes(&self) -> bool {
        self.assume_yes
    }

    /// returns `sql_fun_home` value
    #[must_use]
    pub const fn sql_fun_home(&self) -> &Option<PathBuf> {
        &self.sql_fun_home
    }

    /// returns `engine_versions` values
    #[must_use]
    pub fn engine_versions(&self) -> Vec<String> {
        if let Some(versions) = &self.engine_versions {
            versions.clone()
        } else {
            PostgresVersionInventory::builtin()
                .stable_targets()
                .into_iter()
                .map(|version| version.to_string())
                .collect()
        }
    }

    /// Explicit target selection, before applying source-specific defaults.
    #[must_use]
    pub fn requested_versions(&self) -> Option<&[String]> {
        self.engine_versions.as_deref()
    }

    /// Existing-server source, if configured.
    #[must_use]
    pub fn database_url(&self) -> Option<&str> {
        self.database_url.as_deref()
    }

    /// Address for connecting to Docker-published ports.
    #[must_use]
    pub fn docker_host_address(&self) -> &str {
        &self.docker_host_address
    }

    /// Docker host bind interface.
    #[must_use]
    pub fn docker_bind_address(&self) -> &str {
        &self.docker_bind_address
    }

    /// Per-version extraction timeout.
    #[must_use]
    pub const fn timeout_seconds(&self) -> u64 {
        self.timeout_seconds
    }

    /// print long help
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] : IO error
    ///
    pub fn print_long_help() -> Result<(), std::io::Error> {
        Self::command().print_long_help()
    }
}

#[cfg(test)]
mod test_initialize_args {
    use super::InitializeArgs;
    use ::clap::Parser;

    #[test]
    fn docker_selection_is_optional_and_release_options_are_not_accepted() {
        let args = InitializeArgs::try_parse_from(["init", "--yes", "--engine-versions", "17,18"])
            .unwrap();
        assert_eq!(args.requested_versions().unwrap(), ["17", "18"]);
        assert!(
            InitializeArgs::try_parse_from(["init", "--github-repo", "kazuk/sql-fun"]).is_err()
        );
    }

    #[test]
    fn source_conflicts_do_not_expose_credentials() {
        let error = InitializeArgs::try_parse_from([
            "init",
            "--docker",
            "--database-url",
            "postgres://secret@localhost/postgres",
        ])
        .unwrap_err();
        assert!(!error.to_string().contains("secret"));
    }
}
