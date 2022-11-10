//! Run round-trip source code generation on a given Python file.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use ruff::code_gen::SourceGenerator;
use ruff::source_code_locator::SourceCodeLocator;
use rustpython_ast::Location;
use rustpython_parser::parser;

#[derive(Parser)]
pub struct Cli {
    /// Python file to round-trip.
    #[arg(required = true)]
    file: PathBuf,
}

pub fn main() -> Result<()> {
    let cli = Cli::parse();
    let contents = fs::read_to_string(&cli.file)?;
    let locator = SourceCodeLocator::new(&contents);
    println!("{:?}", locator.slice_source_code_line(&Location::new(3, 0)));
    println!("{:?}", locator.slice_source_code_line(&Location::new(4, 0)));
    println!("{:?}", locator.slice_source_code_line(&Location::new(5, 0)));
    println!("{:?}", locator.slice_source_code_line(&Location::new(6, 0)));
    println!("{:?}", locator.slice_source_code_line(&Location::new(7, 0)));
    println!("{:?}", locator.slice_source_code_line(&Location::new(8, 0)));
    Ok(())
}
