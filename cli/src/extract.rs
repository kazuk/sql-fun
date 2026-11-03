mod bundle;
mod catalog;
mod container;
mod definition_schemas;
mod publication;
mod render;

#[cfg(test)]
mod test_helpers;

use crate::extract::definition_schemas::DefinitionSchemas;

use self::publication::Publication;
use std::{path::PathBuf, process::ExitCode, time::Duration};

/// Extract catalogs, export the administrator script, or import extension files.
#[derive(clap::Args, Debug, Clone)]
pub struct ExtractArgs {
    #[command(subcommand)]
    operation: Operation,
}

#[derive(clap::Subcommand, Debug, Clone)]
enum Operation {
    /// Extract builtin catalogs from a `PostgreSQL` connection or Docker image.
    Catalog(CatalogArgs),
    /// Print the standalone extension export script to standard output.
    ExportScript,
    /// Import an administrator-produced extension bundle into an extracted catalog.
    ImportExtensions(ImportArgs),
}

#[derive(clap::Args, Debug, Clone)]
struct CatalogArgs {
    #[arg(skip)]
    expected_version: Option<sql_fun_core::PostgresVersion>,
    /// `PostgreSQL` connection string; prefer `SQL_FUN_DATABASE_URL` to avoid shell history.
    #[arg(
        long,
        env = "SQL_FUN_DATABASE_URL",
        hide_env_values = true,
        conflicts_with = "docker_image",
        required_unless_present = "docker_image"
    )]
    database_url: Option<String>,
    /// `PostgreSQL` image reference (use a digest for reproducibility).
    #[arg(
        long,
        conflicts_with = "database_url",
        required_unless_present = "database_url"
    )]
    docker_image: Option<String>,
    /// Host address used to reach Docker's published port.
    #[arg(long, default_value = "127.0.0.1", requires = "docker_image")]
    docker_host_address: String,
    /// Address on the Docker host on which to publish the `PostgreSQL` port.
    #[arg(long, default_value = "127.0.0.1", requires = "docker_image")]
    docker_bind_address: String,
    /// Include additional collation/operator-class definitions from this exact schema (repeatable).
    #[arg(long = "definition-schema")]
    definition_schemas: Vec<String>,
    /// Destination containing schema.sql, catalog JSON, and metadata.json.
    #[arg(long)]
    output: PathBuf,
    /// Maximum duration of extraction, including readiness, in seconds.
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u64).range(1..))]
    timeout_seconds: u64,
}

#[derive(clap::Args, Debug, Clone)]
struct ImportArgs {
    /// Uncompressed tar bundle produced by export-script.
    #[arg(long)]
    bundle: PathBuf,
    /// Previously extracted catalog directory, read without modification.
    #[arg(long)]
    catalog: PathBuf,
    /// New or empty directory for the candidate generation.
    #[arg(long)]
    output: PathBuf,
}

impl ExtractArgs {
    pub(crate) async fn execute(&self) -> Result<ExitCode, anyhow::Error> {
        self.operation.execute().await?;
        Ok(ExitCode::SUCCESS)
    }
}

impl Operation {
    async fn execute(&self) -> Result<(), anyhow::Error> {
        match self {
            Self::Catalog(args) => args.execute().await,
            Self::ExportScript => {
                use std::io::Write;
                std::io::stdout()
                    .lock()
                    .write_all(bundle::EXPORT_SCRIPT.as_bytes())?;
                Ok(())
            }
            Self::ImportExtensions(args) => {
                let stage = Publication::new(&args.output)?;
                stage.copy_catalog(&args.catalog)?;
                bundle::import(&args.bundle, stage.path())?;
                stage.publish()
            }
        }
    }
}

pub(crate) async fn detect_server(
    connection: &str,
    seconds: u64,
) -> Result<sql_fun_core::PostgresVersion, anyhow::Error> {
    tokio::time::timeout(Duration::from_secs(seconds), catalog::detect(connection))
        .await
        .map_err(|_| anyhow::anyhow!("PostgreSQL version detection timed out"))?
}

pub(crate) async fn initialize_catalog(
    args: &sql_fun_core::InitializeArgs,
    version: &sql_fun_core::PostgresVersion,
    output: &std::path::Path,
) -> Result<(), anyhow::Error> {
    let source = CatalogArgs {
        expected_version: Some(*version),
        database_url: args.database_url().map(str::to_owned),
        docker_image: if args.database_url().is_none() {
            Some(
                version
                    .extraction_image()
                    .map_err(anyhow::Error::msg)?
                    .to_owned(),
            )
        } else {
            None
        },
        docker_host_address: args.docker_host_address().to_owned(),
        docker_bind_address: args.docker_bind_address().to_owned(),
        definition_schemas: Vec::new(),
        output: output.to_path_buf(),
        timeout_seconds: args.timeout_seconds(),
    };
    source.execute().await
}

impl CatalogArgs {
    async fn execute(&self) -> Result<(), anyhow::Error> {
        let schemas = DefinitionSchemas::new(&self.definition_schemas)?;
        which::which("psql").map_err(|_| {
            anyhow::anyhow!(
                "psql is required for catalog extraction; install the PostgreSQL client tools"
            )
        })?;
        let stage = Publication::new(&self.output)?;
        let duration = Duration::from_secs(self.timeout_seconds);
        // On interruption, finish bounded provisioning so any created resource can be removed.
        let mut container = if let Some(image) = &self.docker_image {
            let creation = container::Container::create(image, &self.docker_bind_address);
            tokio::pin!(creation);
            Some(tokio::select! {
                result = &mut creation => result?,
                signal = interrupted() => {
                    let mut instance = creation.await?;
                    instance.remove().await?;
                    signal?;
                    anyhow::bail!("extraction interrupted during provisioning; previous output preserved");
                }
            })
        } else {
            None
        };
        let extraction = async {
            let (connection, provenance) = if let Some(instance) = &mut container {
                instance.start().await?;
                let connection = instance.connection(&self.docker_host_address).await?;
                catalog::wait_ready(&connection).await?;
                (connection, instance.provenance().await?)
            } else {
                (
                    self.database_url.clone().unwrap_or_default(),
                    serde_json::json!({"source": "connection"}),
                )
            };
            if let Some(expected) = self.expected_version {
                let actual = catalog::detect(&connection).await?;
                anyhow::ensure!(
                    actual == expected,
                    "expected PostgreSQL {expected}, connected server reports {actual}"
                );
            }
            catalog::extract(&connection, stage.path(), &provenance, &schemas).await?;
            if let Some(instance) = &container {
                let bundle_path = stage.path().join("export.tar");
                instance.export(&bundle_path).await?;
                bundle::import(&bundle_path, stage.path())?;
                std::fs::remove_file(bundle_path)?;
            }
            Ok::<(), anyhow::Error>(())
        };
        let result = tokio::select! {
            result = tokio::time::timeout(duration, extraction) => {
                result.unwrap_or_else(|_| Err(anyhow::anyhow!("extraction timed out; previous output preserved")))
            }
            result = interrupted() => {
                result.map_err(anyhow::Error::from).and_then(|()| Err(anyhow::anyhow!("extraction interrupted; previous output preserved")))
            }
        };
        let cleanup = if let Some(instance) = &mut container {
            instance.remove().await
        } else {
            Ok(())
        };
        // Both failures matter: never silently hide a resource left for cleanup.
        if let Err(error) = cleanup {
            return Err(error.context("could not clean up task-owned PostgreSQL container"));
        }
        result?;
        stage.publish()
    }
}

async fn interrupted() -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await
}
