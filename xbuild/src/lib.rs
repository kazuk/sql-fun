//! internal build utilities for `sql-fun`

mod protobuf_prost;
mod public_api_diff;

use crate::{
    protobuf_prost::{EnumTypesCommand, StructTypesCommand},
    public_api_diff::PublicApiDiffCommand,
};

/// command line args
#[derive(clap::Parser)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    EnumTypes(EnumTypesCommand),
    StructTypes(StructTypesCommand),
    PublicApiDiff(PublicApiDiffCommand),
}

impl Args {
    /// # Errors
    ///
    /// - `anyhow::Error`
    pub fn run(&self) -> Result<(), anyhow::Error> {
        self.command.run(self)
    }
}

impl Command {
    fn run(&self, args: &Args) -> Result<(), anyhow::Error> {
        match self {
            Self::EnumTypes(e) => e.run(args)?,
            Self::StructTypes(e) => e.run(args)?,
            Self::PublicApiDiff(command) => command.run()?,
        }
        Ok(())
    }
}
