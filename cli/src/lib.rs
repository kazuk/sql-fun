#![deny(missing_docs)]
//! sql-fun cli binary louncher
mod child_proc;
mod cli_sub_command;
mod docker;
mod init;
mod runnable;

use self::init::initialize;

use ::sql_fun_core::SqlFunArgs;
use std::process::ExitCode;

pub use self::{
    check::CheckArgs,
    cli_sub_command::CliSubCommand,
    runnable::{RunnableCommand, StdStream},
};

mod check;

/// `sql-fun` Command line tools
#[derive(clap::Parser)]
pub struct SqlFunCliArgs {
    #[clap(flatten)]
    common_args: SqlFunArgs,

    /// Subcommand to execute (internal or external)
    #[clap(subcommand)]
    command: Option<CliSubCommand>,
}

impl SqlFunCliArgs {
    /// returns true for builtin subcommand
    #[must_use]
    pub fn is_builtin(&self) -> bool {
        matches!(
            self.command,
            Some(CliSubCommand::Initialize(_) | CliSubCommand::Check(_))
        )
    }

    /// get command
    #[must_use]
    pub fn command(&self) -> &Option<CliSubCommand> {
        &self.command
    }
}

#[cfg(test)]
/// Test helpers for the sql-fun CLI crate.
pub mod test_helpers;

/// Runs builtin CLI subcommands
///
/// # Errors
///
/// Returns an error if executing the chosen subcommand fails.
pub async fn run_builtin(args: &SqlFunCliArgs) -> Result<ExitCode, anyhow::Error> {
    let Some(sub_command) = args.command() else {
        eprintln!("no sub-command specified");
        return Ok(ExitCode::SUCCESS);
    };
    match sub_command {
        CliSubCommand::Initialize(init_args) => Ok(initialize(&args.common_args, init_args).await?),
        CliSubCommand::Check(check_args) => Ok(check_args.execute(&args.common_args).await?),
        _ => unreachable!(),
    }
}
