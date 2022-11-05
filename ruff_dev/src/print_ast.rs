//! Print the AST for a given Python file.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use rustpython_parser::parser;

#[derive(Args)]
pub struct Cli {
    /// Python file for which to generate the AST.
    #[arg(required = true)]
    file: PathBuf,
}

pub fn main(cli: &Cli) -> Result<()> {
    let contents = fs::read_to_string(&cli.file)?;
    let python_ast = parser::parse_program(&contents, &cli.file.to_string_lossy())?;
    println!("{:#?}", python_ast);
    Ok(())
}
