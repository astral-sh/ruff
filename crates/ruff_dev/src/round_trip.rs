//! Run round-trip source code generation on a given Python or Jupyter notebook file.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;

use ruff_python_ast::PySourceType;
use ruff_python_codegen::round_trip;

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Python or Jupyter notebook file to round-trip.
    #[arg(required = true)]
    file: PathBuf,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let path = args.file.as_path();
    if PySourceType::from(path).is_ipynb() {
        println!("{}", ruff_notebook::round_trip(path)?);
    } else {
        let contents = fs::read_to_string(&args.file)?;
        println!("{}", round_trip(&contents)?);
    }
    Ok(())
}
