mod protobuf_prost;

#[derive(clap::Parser)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    EnumTypes(self::protobuf_prost::EnumTypesCommand),
    StructTypes(self::protobuf_prost::StructTypesCommand),
}

impl Args {
    pub fn run(&self) -> Result<(), anyhow::Error> {
        self.command.run(self)
    }
}

impl Command {
    fn run(&self, args: &Args) -> Result<(), anyhow::Error> {
        match self {
            Self::EnumTypes(e) => e.run(args)?,
            Self::StructTypes(e) => e.run(args)?,
        }
        Ok(())
    }
}
