//! Run round-trip source code generation on a given Python file.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use ruff::code_gen::SourceGenerator;
use rustpython_parser::parser;

#[derive(Args)]
pub struct Cli {
    /// Python file to round-trip.
    #[arg(required = true)]
    file: PathBuf,
}

pub fn main(cli: &Cli) -> Result<()> {
    let contents = fs::read_to_string(&cli.file)?;
    let python_ast = parser::parse_program(&contents, &cli.file.to_string_lossy())?;
    let mut generator = SourceGenerator::new();
    generator.unparse_suite(&python_ast)?;
    println!("{}", generator.generate()?);
    Ok(())
}
