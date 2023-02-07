//! Run round-trip source code generation on a given Python file.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use ruff::source_code::round_trip;

#[derive(clap::Args)]
pub struct Args {
    /// Python file to round-trip.
    #[arg(required = true)]
    file: PathBuf,
}

pub fn main(args: &Args) -> Result<()> {
    let contents = fs::read_to_string(&args.file)?;
    println!("{}", round_trip(&contents, &args.file.to_string_lossy())?);
    Ok(())
}
