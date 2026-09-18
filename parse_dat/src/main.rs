//! parse postgres dat file
//!
use clap::Parser;

fn main() -> Result<(), parse_dat::ParseDatError> {
    let args = parse_dat::ParseDatArgs::parse();
    args.run()?;
    Ok(())
}
