use std::path::PathBuf;

use clap::{CommandFactory, ValueHint};

use url::Url;

/// Initialize `sql-fun` environments
#[derive(clap::Parser, Debug, Clone)]
pub struct InitializeArgs {
    /// Disable prompts (assume yes)
    ///
    /// Run non-interactively. Assume "yes" to all prompts, including creating
    /// directories and overwriting files when required.
    ///
    /// Use with care.
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

    /// Database connection URL (e.g., <postgres://user:pass@host:5432/dbname>).
    ///
    /// If provided, `sql-fun` will connect to the database and build the
    /// built-in object catalog directly from the live server.
    ///
    /// This bypasses artifact download (see --github-repo / --sql-fun-version).
    ///
    /// [default: download `sql-fun` release artifact and use it. ]
    ///
    #[arg(long, conflicts_with_all = ["github_repo", "sql_fun_version"])]
    database_url: Option<Url>,

    /// Source repository (owner/repo) for release artifacts containing
    ///   prebuilt catalogs and extension metadata.
    #[arg(long, default_value = "kazuk/sql-fun")]
    github_repo: String,

    /// `sql-fun` version to use when downloading artifacts (semver).
    ///
    /// If omitted, the latest stable release is used.
    ///
    /// [default: latest stable release]
    #[arg(long)]
    sql_fun_version: Option<String>,

    /// Limit initialization to the specified database engine versions.
    ///
    /// Accepts multiple values (repeatable) or comma-separated lists.
    ///
    /// [default: install ALL engine versions in artifact/source]
    ///
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    engine_versions: Option<Vec<String>>,

    /// Limit initialization to the specified `PostgreSQL` extensions.
    ///
    /// Accepts multiple values (repeatable) or comma-separated lists.
    ///
    /// [default: install ALL extensions in artifact/source]
    ///
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    postgres_extensions: Option<Vec<String>>,
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
            vec![String::from("16"), String::from("17"), String::from("18")]
        }
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
