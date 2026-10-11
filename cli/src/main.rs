//! sql-fun Command line interface

use std::process::ExitCode;

use sql_fun_cli::{RunnableCommand, SqlFunCliArgs};

/// execution entry
///
/// # Errors
///
/// Returns an error if command parsing, construction, or execution fails.
///
pub fn main() -> Result<ExitCode, anyhow::Error> {
    let runtime = ::tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(run());
    // Tokio stdin uses a blocking read that cannot be cancelled. A terminal
    // transport/output failure must exit even while the caller keeps stdin open.
    // API completion has already drained all accepted callbacks before this point.
    runtime.shutdown_background();
    result
}

async fn run() -> Result<ExitCode, anyhow::Error> {
    let args = SqlFunCliArgs::parse_with_sources();

    let exit_code = if args.is_builtin() {
        sql_fun_cli::run_builtin(&args).await?
    } else if let Some(command) = args.command() {
        let bin = RunnableCommand::try_from(command)?;
        bin.execute(&args).await?
    } else {
        eprintln!("no sub-command specified");
        ExitCode::SUCCESS
    };

    Ok(exit_code)
}
