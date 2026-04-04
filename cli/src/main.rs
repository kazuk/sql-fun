use clap::Parser;
use std::process::ExitCode;

use sql_fun_cli::{RunableCommand, SqlFunCliArgs};

#[tokio::main]
pub async fn main() -> Result<ExitCode, anyhow::Error> {
    let args = SqlFunCliArgs::parse();

    let exit_code = if args.is_builtin() {
        sql_fun_cli::run_builtin(&args).await?
    } else if let Some(command) = args.command() {
        let bin = RunableCommand::try_from(command)?;
        bin.execute(&args).await?
    } else {
        eprintln!("no sub-command specified");
        ExitCode::SUCCESS
    };

    Ok(exit_code)
}
