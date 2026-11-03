use std::path::PathBuf;

use clap::{CommandFactory, ValueHint};

/// Initialize `sql-fun` environments
#[derive(clap::Parser, Debug, Clone)]
pub struct InitializeArgs {
    /// Complete setup without prompts when all required choices are supplied
    ///
    /// Required choices must be supplied through CLI parameters. Missing choices
    /// are errors; this flag does not select defaults. Existing installations
    /// are never overwritten. Supply --sql-fun-home and either --engines or
    /// --datasource. `PostgreSQL` also requires --docker or --database-url, and
    /// --engine-versions. A missing theme requires --install-default-theme;
    /// a valid installed theme is retained without downloading.
    ///
    ///
    #[arg(long = "yes", short = 'y')]
    assume_yes: bool,

    /// `sql-fun` Home directory path. (optional)
    ///
    /// Path to the sql-fun home directory.
    ///
    /// This is where catalogs, extensions, and configuration files are stored.
    /// Accepts absolute paths, paths relative to the current working directory,
    /// and paths beginning with ~/.
    /// If omitted, ask interactively. `SQL_FUN_HOME` supplies a suggested candidate;
    /// if it is unset, suggest ~/.sql-fun. The user must explicitly confirm the path.
    ///
    /// With --yes, this CLI argument is required; `SQL_FUN_HOME` does not replace it.
    ///
    #[arg(long, short = 'H', value_hint = ValueHint::DirPath)]
    sql_fun_home: Option<PathBuf>,

    /// Download the published default theme when no installed theme exists.
    /// Never overwrites an existing theme. Required with --yes if the theme is missing.
    #[arg(long)]
    install_default_theme: bool,

    /// Engines whose development support should be initialized.
    /// If omitted, ask for engines interactively unless project access was requested.
    #[arg(long, value_delimiter = ',', num_args = 1.., value_parser = ["postgres", "sqlite"])]
    engines: Option<Vec<String>>,

    /// Project datasources explicitly selected for initialization.
    /// Requests project metadata access. Accepts comma-separated names;
    /// does not select from environment defaults.
    #[arg(long = "datasource", value_delimiter = ',', num_args = 1.., conflicts_with = "engines")]
    datasources: Option<Vec<String>>,

    /// Extract `PostgreSQL` builtins from an explicitly selected existing server.
    /// Uses libpq connection strings; prefer password files to inline credentials.
    #[arg(long, hide_env_values = true, conflicts_with = "docker")]
    database_url: Option<String>,

    /// Extract each selected version using its registered `PostgreSQL` Docker image.
    #[arg(long, conflicts_with = "database_url")]
    docker: bool,

    /// Address used to reach published `PostgreSQL` ports on the Docker host.
    ///
    /// Default: 127.0.0.1.
    #[arg(long, conflicts_with = "database_url")]
    docker_host_address: Option<String>,

    /// Docker host interface on which `PostgreSQL` ports are published.
    ///
    /// Default: 127.0.0.1 (loopback only).
    #[arg(long, conflicts_with = "database_url")]
    docker_bind_address: Option<String>,

    /// Extraction timeout in seconds for each selected version.
    ///
    /// Default: 120 seconds.
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    timeout_seconds: Option<u64>,

    /// Explicitly select the database engine versions to initialize.
    /// Currently applies to `PostgreSQL` setup only.
    ///
    /// Accepts space-separated values, comma-separated lists, or repeated options.
    ///
    /// Supported `PostgreSQL` targets: 17, 18. If omitted, ask interactively.
    /// Required with --yes when `PostgreSQL` setup is selected.
    ///
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    engine_versions: Option<Vec<String>>,
}

impl InitializeArgs {
    /// Whether downloading the missing default theme was explicitly authorized.
    #[must_use]
    pub const fn install_default_theme(&self) -> bool {
        self.install_default_theme
    }

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

    /// Explicit `PostgreSQL` target selection; omission never selects defaults.
    #[must_use]
    pub fn requested_versions(&self) -> Option<&[String]> {
        self.engine_versions.as_deref()
    }

    /// Whether any PostgreSQL-specific setup option was supplied.
    #[must_use]
    pub const fn has_postgres_options(&self) -> bool {
        self.docker
            || self.database_url.is_some()
            || self.engine_versions.is_some()
            || self.docker_host_address.is_some()
            || self.docker_bind_address.is_some()
            || self.timeout_seconds.is_some()
    }

    /// Explicit engine selection.
    #[must_use]
    pub fn engines(&self) -> Option<&[String]> {
        self.engines.as_deref()
    }

    /// Explicit datasource selection for this setup request.
    #[must_use]
    pub fn datasources(&self) -> Option<&[String]> {
        self.datasources.as_deref()
    }

    /// Whether the Docker source was explicitly selected.
    #[must_use]
    pub const fn docker(&self) -> bool {
        self.docker
    }

    /// Apply the Docker source chosen interactively.
    pub const fn select_docker(&mut self) {
        self.docker = true;
    }

    /// Apply an existing-server source chosen interactively.
    pub fn set_database_url(&mut self, connection: &str) {
        self.database_url = Some(connection.to_owned());
    }

    /// Existing-server source, if configured.
    #[must_use]
    pub fn database_url(&self) -> Option<&str> {
        self.database_url.as_deref()
    }

    /// Address for connecting to Docker-published ports.
    #[must_use]
    pub fn docker_host_address(&self) -> &str {
        self.docker_host_address.as_deref().unwrap_or("127.0.0.1")
    }

    /// Docker host bind interface.
    #[must_use]
    pub fn docker_bind_address(&self) -> &str {
        self.docker_bind_address.as_deref().unwrap_or("127.0.0.1")
    }

    /// Per-version extraction timeout.
    #[must_use]
    pub const fn timeout_seconds(&self) -> u64 {
        match self.timeout_seconds {
            Some(seconds) => seconds,
            None => 120,
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

#[cfg(test)]
mod test_initialize_args {
    use crate::args::init_args::InitializeArgs;
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

    #[rstest::rstest]
    #[case::separate_values(&["--engine-versions", "17", "18"])]
    #[case::comma_separated(&["--engine-versions", "17,18"])]
    #[case::repeated_option(&["--engine-versions", "17", "--engine-versions", "18"])]
    fn explicit_version_formats_preserve_all_targets(#[case] options: &[&str]) {
        let args =
            InitializeArgs::try_parse_from(std::iter::once("init").chain(options.iter().copied()))
                .unwrap();
        assert_eq!(args.requested_versions().unwrap(), ["17", "18"]);
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
