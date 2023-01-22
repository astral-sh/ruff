//! Run round-trip source code generation on a given Python file.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use ruff::source_code::round_trip;

#[derive(Args)]
pub struct Cli {
    /// Python file to round-trip.
    #[arg(required = true)]
    file: PathBuf,
}

pub fn main(cli: &Cli) -> Result<()> {
    let contents = fs::read_to_string(&cli.file)?;
    println!("{}", round_trip(&contents, &cli.file.to_string_lossy())?);
    Ok(())
}
