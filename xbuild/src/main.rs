use clap::Parser;
use xbuild::Args;

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    args.run()
}
