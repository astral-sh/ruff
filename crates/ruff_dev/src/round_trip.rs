//! Run round-trip source code generation on a given Python or Jupyter notebook file.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::PathBuf;

use anyhow::Result;

use ruff_python_codegen::round_trip;
use ruff_python_stdlib::path::is_jupyter_notebook;

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Python or Jupyter notebook file to round-trip.
    #[arg(required = true)]
    file: PathBuf,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let path = args.file.as_path();
    if is_jupyter_notebook(path) {
        println!("{}", ruff_notebook::round_trip(path)?);
    } else {
        let contents = fs::read_to_string(&args.file)?;
        println!("{}", round_trip(&contents)?);
    }
    Ok(())
}
