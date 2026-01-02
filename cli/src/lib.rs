#![deny(missing_docs)]
//! sql-fun cli binary louncher
mod child_proc;
mod docker;
mod init;
mod runnable;

use self::init::initialize;
use ::sql_fun_core::{CliSubCommand, SqlFunArgs};
use std::process::ExitCode;

pub use self::runnable::{RunableCommand, StdStream};

/// Runs builtin CLI subcommands
///
/// # Errors
///
/// Returns an error if executing the chosen subcommand fails.
pub async fn run_builtin(args: &SqlFunArgs) -> Result<ExitCode, anyhow::Error> {
    let sub_command = args.cli_sub_command();
    match sub_command {
        CliSubCommand::Initialize(init_args) => Ok(initialize(args, init_args).await?),
        _ => unreachable!(),
    }
}
