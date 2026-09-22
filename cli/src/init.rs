mod database_engine_versions;
mod error;
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
    _args: &SqlFunArgs,
    init_args: &InitializeArgs,
) -> Result<ExitCode, InitError> {
    let mut context = Initialization::from(init_args);
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
    todo!();
}
