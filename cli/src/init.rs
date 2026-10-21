mod error;
mod install;
mod postgres;
mod presentation;
mod prompt;
mod selection;
mod sqlite;
mod workflow;

use self::{
    error::InitError,
    postgres::PostgresSetup,
    workflow::{Initialization, Workflow},
};
use ::sql_fun_core::{InitializeArgs, SqlFunArgs};
use ::statig::blocking::IntoStateMachineExt;
use std::{path::Path, process::ExitCode};

/// Initialize explicitly selected engine-specific development support.
///
/// # Errors
/// Reports incomplete choices, invalid selected configuration or setup failures.
pub async fn initialize(
    args: &SqlFunArgs,
    init_args: &InitializeArgs,
    metadata_file_from_cli: bool,
) -> Result<ExitCode, anyhow::Error> {
    let mut context = Initialization::new(args, init_args, metadata_file_from_cli)?;
    if !init_args.assume_yes() {
        println!("{}", include_str!("init/banner.txt"));
    }
    let mut machine = Workflow.state_machine();
    while machine.state().is_asking() {
        let event = context.prompt().await?;
        machine.handle_with_context(&event, &mut context);
    }
    let exit = context.finish()?;
    if machine.state().is_authorized() {
        context.install().await?;
    }
    Ok(exit)
}

enum EngineSetup {
    Postgres(Box<PostgresSetup>),
    Sqlite,
}
impl EngineSetup {
    fn postgres(setup: &PostgresSetup) -> Self {
        Self::Postgres(Box::new(setup.clone()))
    }
    const fn sqlite() -> Self {
        Self::Sqlite
    }
    async fn install(&self, home: &Path) -> Result<(), InitError> {
        match self {
            Self::Postgres(setup) => setup.install(home).await,
            Self::Sqlite => {
                sqlite::initialize(home);
                Ok(())
            }
        }
    }
}
