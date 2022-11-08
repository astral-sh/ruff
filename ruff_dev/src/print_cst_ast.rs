//! Print the LibCST AST for a given Python file.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct Cli {
    /// Python file for which to generate the AST.
    #[arg(required = true)]
    file: PathBuf,
}

pub fn main(cli: &Cli) -> Result<()> {
    let contents = fs::read_to_string(&cli.file)?;
    match libcst_native::parse_module(&contents, None) {
        Ok(m) => println!("{:#?}", m),
        Err(_) => {}
    }
    Ok(())
}
