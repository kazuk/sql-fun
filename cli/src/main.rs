use std::process::ExitCode;

use sql_fun_cli::RunableCommand;
use sql_fun_core::SqlFunArgs;

#[tokio::main]
pub async fn main() -> Result<ExitCode, anyhow::Error> {
    let args = SqlFunArgs::parse_args();

    let exit_code = if args.is_builtin() {
        sql_fun_cli::run_builtin(&args).await?
    } else {
        let bin = RunableCommand::try_from(args.command())?;
        bin.execute(&args).await?
    };

    Ok(exit_code)
}
