mod prepare_home;
use self::prepare_home::prepare_home;

use sql_fun_core::{InitializeArgs, SqlFunArgs};
use std::process::ExitCode;

/// `sqlfun init` sub command routine
pub async fn initialize(
    _args: &SqlFunArgs,
    init_args: &InitializeArgs,
) -> Result<ExitCode, anyhow::Error> {
    let yes = init_args.assume_yes();

    if !yes {
        let banner = include_str!("init/banner.txt");
        println!("{banner}");

        let message = "Install `sql-fun`?";
        let install = inquire::Confirm::new(message).prompt()?;
        if !install {
            return Ok(ExitCode::FAILURE);
        }
    }
    let home_arg = init_args.sql_fun_home();
    let _home = prepare_home(yes, home_arg.as_ref())?;

    todo!();
}
