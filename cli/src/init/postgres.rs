use crate::{
    extract::detect_server,
    init::{choice, error::InitError, install, prompt::PromptRequest, step::PromptInput},
};

use std::path::Path;

use ::sql_fun_core::{
    InitializationDatasources, InitializeArgs, PostgresVersion, PostgresVersionInventory,
    SqlFunArgs,
};

/// A `PostgreSQL` setup task with all choices resolved before installation.
#[derive(Clone)]
pub(super) struct PostgresSetup {
    args: InitializeArgs,
    versions: Vec<PostgresVersion>,
}

impl PostgresSetup {
    pub(super) fn check_choices(args: &InitializeArgs) -> Result<(), anyhow::Error> {
        if args.assume_yes() {
            anyhow::ensure!(
                args.docker() || args.database_url().is_some(),
                "--yes requires --docker or --database-url for PostgreSQL setup"
            );
            anyhow::ensure!(
                args.requested_versions().is_some(),
                "--yes requires --engine-versions for PostgreSQL setup"
            );
        }
        Ok(())
    }

    pub(super) fn from_choices(
        common: &SqlFunArgs,
        args: &InitializeArgs,
        versions: &[PostgresVersion],
        metadata: Option<&InitializationDatasources>,
        datasource_names: &[String],
    ) -> Result<Self, anyhow::Error> {
        let setup = Self {
            args: args.clone(),
            versions: versions.to_vec(),
        };
        if let Some(version) = common.postgres_version_override()? {
            setup.validate_version(&version.to_string())?;
        }
        if let Some(metadata) = metadata {
            for name in datasource_names {
                if metadata.engine(name)? == "PostgreSQL" {
                    let selected = metadata.selected_metadata(name)?;
                    setup.validate_version(&common.postgres_version(&selected)?.to_string())?;
                }
            }
        }
        Ok(setup)
    }

    pub(super) fn choose_source(
        args: &InitializeArgs,
        home: &Path,
        retained: Option<bool>,
        input: &mut dyn PromptInput,
    ) -> Result<bool, InitError> {
        let explanation = format!(
            "{}Destination: {}/postgres/<selected-version>/.",
            include_str!("postgres_source.txt"),
            home.display()
        );
        if args.docker() || args.database_url().is_some() {
            if !args.assume_yes() {
                let source = if args.docker() {
                    "--docker: Docker"
                } else {
                    "--database-url: Existing PostgreSQL server (connection value not repeated)"
                };
                println!("PostgreSQL source supplied by {source}.\n{explanation}");
            }
            return Ok(args.database_url().is_some());
        }
        loop {
            let retained = match retained {
                Some(true) => vec!["Existing PostgreSQL server".to_owned()],
                Some(false) => vec!["Docker".to_owned()],
                None => vec![],
            };
            let selected = choice::select_retained(
                "Choose the PostgreSQL builtin source",
                &["Docker".to_owned(), "Existing PostgreSQL server".to_owned()],
                &retained,
                |request| input.answer(&request.clone().with_explanation(&explanation)),
            )?;
            if selected.len() != 1 {
                eprintln!("Choose exactly one source.");
                continue;
            }
            if selected[0] == "Existing PostgreSQL server" {
                return Ok(true);
            }
            return Ok(false);
        }
    }

    pub(super) fn choose_connection(
        args: &InitializeArgs,
        retained: Option<&str>,
        input: &mut dyn PromptInput,
    ) -> Result<String, InitError> {
        if let Some(connection) = args.database_url() {
            if !args.assume_yes() {
                println!(
                    "Using the server connection supplied with --database-url; psql will connect now to validate its version. No installation occurs during this check."
                );
            }
            return Ok(connection.to_owned());
        }
        loop {
            let answer = input.answer(
                &PromptRequest::text(
                    "PostgreSQL libpq connection string (prefer a password file)",
                    "",
                )
                .with_initial_text(retained.unwrap_or(""))
                .with_explanation(include_str!("postgres_connection.txt")),
            )?;
            let Some(connection) = answer.text_input() else {
                continue;
            };
            if connection.trim().is_empty() {
                eprintln!(
                    "Enter a PostgreSQL connection URL or libpq key=value connection string; a server connection is required for this source."
                );
                continue;
            }
            return Ok(connection.to_owned());
        }
    }

    pub(super) async fn choose_versions(
        args: &InitializeArgs,
        home: &Path,
        requirements: &str,
        retained: Option<&[PostgresVersion]>,
        input: &mut dyn PromptInput,
    ) -> Result<Vec<PostgresVersion>, anyhow::Error> {
        let retained = retained
            .into_iter()
            .flatten()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        // Explicit values are validated before connecting or provisioning.
        let versions = if let Some(requested) = args.requested_versions() {
            Some(
                PostgresVersionInventory::builtin()
                    .select(requested)
                    .map_err(anyhow::Error::msg)?,
            )
        } else {
            None
        };
        if !args.assume_yes()
            && let Some(selected) = &versions
        {
            let identities = selected.iter().map(ToString::to_string).collect::<Vec<_>>();
            println!(
                "PostgreSQL versions supplied by --engine-versions: {}.\n{}",
                identities.join(", "),
                Self::version_destinations(home, requirements, &identities)
            );
        }
        let detected = Self::detect_selected_server(args).await?;
        if let Some(selected) = versions {
            if let Some(detected) = detected {
                anyhow::ensure!(
                    selected == [detected],
                    "selected versions {} must match connected PostgreSQL {detected}",
                    selected
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            if !args.assume_yes()
                && let Some(detected) = detected
            {
                println!(
                    "Connected server: PostgreSQL {detected}. Your explicit version selection will be prepared after final Ok."
                );
            }
            return Ok(selected);
        }
        let (title, options) = if let Some(detected) = detected {
            (
                "Choose the connected PostgreSQL version to initialize",
                vec![detected.to_string()],
            )
        } else {
            let options = PostgresVersionInventory::builtin()
                .targets()
                .into_iter()
                .map(|version| version.to_string())
                .collect::<Vec<_>>();
            ("Choose PostgreSQL versions to initialize", options)
        };
        let explanation = Self::version_explanation(args, home, requirements, &options);
        let selected = choice::select_retained(title, &options, &retained, |request| {
            input.answer(&request.clone().with_explanation(&explanation))
        })?;
        PostgresVersionInventory::builtin()
            .select(&selected)
            .map_err(anyhow::Error::msg)
    }

    async fn detect_selected_server(
        args: &InitializeArgs,
    ) -> Result<Option<PostgresVersion>, anyhow::Error> {
        let Some(connection) = args.database_url() else {
            return Ok(None);
        };
        if !args.assume_yes() {
            println!(
                "Checking the version of the PostgreSQL server you selected using psql. Connection details are not repeated here."
            );
        }
        let detected = detect_server(connection, args.timeout_seconds()).await
            .map_err(|error| error.context("Could not validate the selected PostgreSQL server. Check connection settings, psql availability, server support and catalog permissions, then rerun"))?;
        Ok(Some(detected))
    }

    fn version_explanation(
        args: &InitializeArgs,
        home: &Path,
        requirements: &str,
        options: &[String],
    ) -> String {
        let mut lines = Vec::new();
        if args.database_url().is_some() {
            lines.push(format!("Connected server: PostgreSQL {}. Only that server version can be extracted from this connection. Select it explicitly to prepare its metadata after final Ok.", options.join(", ")));
        } else {
            lines.push("Choose PostgreSQL versions your SQL needs to be checked against. Docker will use a temporary server for each selected version after final Ok.".to_owned());
        }
        lines.push(Self::version_destinations(home, requirements, options));
        lines.join("\n")
    }

    fn version_destinations(home: &Path, requirements: &str, options: &[String]) -> String {
        let mut lines =
            vec!["Each selected version gets its own builtin metadata installation:".to_owned()];
        lines.extend(options.iter().map(|version| {
            format!(
                "  {version} -> {}",
                home.join("postgres").join(version).display()
            )
        }));
        if !requirements.is_empty() {
            lines.push(requirements.to_owned());
        }
        lines.join("\n")
    }

    pub(super) fn review(&self, home: &Path, supplied_connection: bool) -> String {
        let mut lines = if self.args.database_url().is_some() {
            vec!["PostgreSQL source: Existing PostgreSQL server".to_owned(),
                format!("Server connection: {}; value not repeated", if supplied_connection { "supplied with --database-url" } else { "entered in this session" }),
                "PostgreSQL operations: use psql to read builtin catalogs from the selected server; Docker is not required.".to_owned()]
        } else {
            vec!["PostgreSQL source: Docker".to_owned(),
                "PostgreSQL operations: use psql and Docker with an accessible daemon; images may need downloading. Start and remove temporary PostgreSQL containers for selected versions.".to_owned()]
        };
        lines.push("PostgreSQL versions and destinations (execution order):".to_owned());
        lines.extend(self.versions.iter().map(|version| {
            format!(
                "  {version} -> {}",
                version.definition_base_path(home).display()
            )
        }));
        lines.push("Existing complete compatible metadata is kept. Incomplete or incompatible metadata causes failure; it is not overwritten.".to_owned());
        lines.join("\n")
    }

    fn validate_version(&self, configured: &str) -> Result<(), anyhow::Error> {
        anyhow::ensure!(
            self.versions
                .contains(&configured.parse().map_err(anyhow::Error::msg)?),
            "configured PostgreSQL {configured} is not included in initialization targets"
        );
        Ok(())
    }

    pub(super) async fn install(&self, home: &Path) -> Result<(), InitError> {
        for version in &self.versions {
            install::install(&self.args, home, version).await.map_err(|error| error.context(format!("initialization failed for PostgreSQL {version}; completed versions remain installed")))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_choices {
    use crate::init::postgres::PostgresSetup;
    use ::clap::Parser;
    use ::sql_fun_core::InitializeArgs;

    #[rstest::rstest]
    #[case::missing_source(&["--yes", "--engine-versions", "17"], "--docker or --database-url")]
    #[case::missing_versions(&["--yes", "--docker"], "--engine-versions")]
    fn omissions_are_not_consent(#[case] options: &[&str], #[case] missing: &str) {
        let args =
            InitializeArgs::try_parse_from(std::iter::once("init").chain(options.iter().copied()))
                .unwrap();
        assert!(
            PostgresSetup::check_choices(&args)
                .unwrap_err()
                .to_string()
                .contains(missing)
        );
    }
}
