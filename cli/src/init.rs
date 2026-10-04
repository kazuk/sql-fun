mod database_engine_versions;
mod error;
mod install;
mod presentation;
mod prompt;
mod workflow;

use self::{
    error::InitError,
    workflow::{Initialization, Workflow},
};

use std::process::ExitCode;

use ::sql_fun_core::{InitializeArgs, SqlFunArgs};
use ::statig::blocking::IntoStateMachineExt;

/// `sqlfun init` sub command routine
pub async fn initialize(
    args: &SqlFunArgs,
    init_args: &InitializeArgs,
) -> Result<ExitCode, InitError> {
    let mut context = Initialization::from(init_args);
    if init_args.sql_fun_home().is_none() {
        context.set_home(&args.initialization_home().map_err(anyhow::Error::from)?);
    }
    let server = if let Some(connection) = init_args.database_url() {
        let version =
            crate::extract::detect_server(connection, init_args.timeout_seconds()).await?;
        if let Some(requested) = init_args.requested_versions() {
            let selected = sql_fun_core::PostgresVersionInventory::builtin()
                .select(requested)
                .map_err(anyhow::Error::msg)?;
            if selected != [version] {
                return Err(anyhow::anyhow!(
                    "selected versions must match connected PostgreSQL {version}"
                )
                .into());
            }
        }
        context.use_server_version(&version);
        Some(version)
    } else {
        None
    };
    let mut machine = Workflow.state_machine();
    if !init_args.assume_yes() {
        println!("{}", include_str!("init/banner.txt"));
    }
    while let Some(request) = machine.state().request(&context) {
        let event = request.run()?;
        machine.handle_with_context(&event, &mut context);
        context.present();
    }
    context.finish()?;
    if machine.state().is_cancelled() {
        return Ok(ExitCode::FAILURE);
    }
    let versions = if let Some(version) = server {
        vec![version]
    } else {
        let selected = context
            .versions()
            .into_iter()
            .map(|version| version.to_string())
            .collect::<Vec<_>>();
        sql_fun_core::PostgresVersionInventory::builtin()
            .select(&selected)
            .map_err(anyhow::Error::msg)?
    };
    validate_project(args, &versions)?;
    for version in versions {
        install::install(init_args, context.home(), &version).await
            .map_err(|error| error.context(format!("initialization failed for PostgreSQL {version}; completed versions remain installed")))?;
    }
    Ok(ExitCode::SUCCESS)
}

fn validate_project(
    args: &SqlFunArgs,
    versions: &[sql_fun_core::PostgresVersion],
) -> Result<(), anyhow::Error> {
    if let Some(version) = args.postgres_version_override()? {
        validate_target_version(&version.to_string(), versions)?;
    }
    // Standalone initialization may omit a project, but explicit metadata must resolve.
    if args.metadata_file().is_err() && !args.metadata_file_is_explicit() {
        args.metadata_file_raw()?;
        return Ok(());
    }
    let metadata = args.load_metadata()?;
    let name = args.datasource().unwrap_or("default");
    let selected = metadata
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("configured datasource {name} not found"))?;
    if args.sql_dialect(selected)?.to_string() != "postgresql" {
        return Ok(());
    }
    let configured = args.postgres_version(selected)?.to_string();
    validate_target_version(&configured, versions)
}

fn validate_target_version(
    configured: &str,
    versions: &[sql_fun_core::PostgresVersion],
) -> Result<(), anyhow::Error> {
    anyhow::ensure!(
        versions.contains(&configured.parse().map_err(anyhow::Error::msg)?),
        "configured PostgreSQL {configured} is not included in initialization targets"
    );
    Ok(())
}
