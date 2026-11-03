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
        retained: Option<bool>,
        input: &mut dyn PromptInput,
    ) -> Result<bool, InitError> {
        if args.docker() || args.database_url().is_some() {
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
                |request| input.answer(request),
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
            return Ok(connection.to_owned());
        }
        loop {
            let answer = input.answer(
                &PromptRequest::text(
                    "PostgreSQL libpq connection string (prefer a password file)",
                    "",
                )
                .with_initial_text(retained.unwrap_or("")),
            )?;
            let Some(connection) = answer.text_input() else {
                continue;
            };
            if connection.trim().is_empty() {
                continue;
            }
            return Ok(connection.to_owned());
        }
    }

    pub(super) async fn choose_versions(
        args: &InitializeArgs,
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
        let (title, options) = if let Some(connection) = args.database_url() {
            let detected = detect_server(connection, args.timeout_seconds()).await?;
            if let Some(selected) = versions {
                anyhow::ensure!(
                    selected == [detected],
                    "selected versions must match connected PostgreSQL {detected}"
                );
                return Ok(selected);
            }
            (
                "Choose the connected PostgreSQL version to initialize",
                vec![detected.to_string()],
            )
        } else {
            if let Some(selected) = versions {
                return Ok(selected);
            }
            let options = PostgresVersionInventory::builtin()
                .targets()
                .into_iter()
                .map(|version| version.to_string())
                .collect::<Vec<_>>();
            ("Choose PostgreSQL versions to initialize", options)
        };
        let selected =
            choice::select_retained(title, &options, &retained, |request| input.answer(request))?;
        PostgresVersionInventory::builtin()
            .select(&selected)
            .map_err(anyhow::Error::msg)
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
