use super::{choice, error::InitError, install, prompt::PromptRequest};

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

    pub(super) async fn prepare(
        common: &SqlFunArgs,
        args: &InitializeArgs,
        metadata: Option<&InitializationDatasources>,
        datasource_names: &[String],
    ) -> Result<Self, anyhow::Error> {
        Self::check_choices(args)?;
        let mut resolved = args.clone();
        if !args.docker() && args.database_url().is_none() {
            loop {
                let selected = choice::select(
                    "Choose the PostgreSQL builtin source",
                    &["Docker".to_owned(), "Existing PostgreSQL server".to_owned()],
                )?;
                if selected.len() != 1 {
                    eprintln!("Choose exactly one source.");
                    continue;
                }
                if selected[0] == "Existing PostgreSQL server" {
                    loop {
                        let answer = PromptRequest::text(
                            "PostgreSQL libpq connection string (prefer a password file)",
                            "",
                        )
                        .run()?;
                        let Some(connection) = answer.text_input() else {
                            continue;
                        };
                        if connection.trim().is_empty() {
                            continue;
                        }
                        resolved.set_database_url(connection);
                        break;
                    }
                } else {
                    resolved.select_docker();
                }
                break;
            }
        }
        // Explicit values are validated before connecting or provisioning.
        let mut versions = if let Some(requested) = args.requested_versions() {
            Some(
                PostgresVersionInventory::builtin()
                    .select(requested)
                    .map_err(anyhow::Error::msg)?,
            )
        } else {
            None
        };
        if let Some(connection) = resolved.database_url() {
            let detected =
                crate::extract::detect_server(connection, args.timeout_seconds()).await?;
            if let Some(selected) = &versions {
                anyhow::ensure!(
                    selected == &[detected],
                    "selected versions must match connected PostgreSQL {detected}"
                );
            } else {
                // Detection constrains the choices; it does not choose on the user's behalf.
                let selected = choice::select(
                    "Choose the connected PostgreSQL version to initialize",
                    &[detected.to_string()],
                )?;
                versions = Some(
                    PostgresVersionInventory::builtin()
                        .select(&selected)
                        .map_err(anyhow::Error::msg)?,
                );
            }
        } else if versions.is_none() {
            let options = PostgresVersionInventory::builtin()
                .targets()
                .into_iter()
                .map(|version| version.to_string())
                .collect::<Vec<_>>();
            let selected = choice::select("Choose PostgreSQL versions to initialize", &options)?;
            versions = Some(
                PostgresVersionInventory::builtin()
                    .select(&selected)
                    .map_err(anyhow::Error::msg)?,
            );
        }
        let setup = Self {
            args: resolved,
            versions: versions.expect("explicit version choice"),
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
    use super::PostgresSetup;
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
