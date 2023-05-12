//! Print the `LibCST` CST for a given Python file.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Result};

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Python file for which to generate the CST.
    #[arg(required = true)]
    file: PathBuf,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let contents = fs::read_to_string(&args.file)?;
    match libcst_native::parse_module(&contents, None) {
        Ok(python_cst) => {
            println!("{python_cst:#?}");
            Ok(())
        }
        Err(_) => bail!("Failed to parse CST"),
    }
}
