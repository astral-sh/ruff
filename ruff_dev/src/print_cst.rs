//! Print the `LibCST` CST for a given Python file.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct Cli {
    /// Python file for which to generate the CST.
    #[arg(required = true)]
    file: PathBuf,
}

pub fn main(cli: &Cli) -> Result<()> {
    let contents = fs::read_to_string(&cli.file)?;
    match libcst_native::parse_module(&contents, None) {
        Ok(python_cst) => {
            println!("{python_cst:#?}");
            Ok(())
        }
        Err(_) => Err(anyhow::anyhow!("Failed to parse CST")),
    }
}
