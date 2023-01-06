//! Run round-trip source code generation on a given Python file.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use ruff::source_code_generator::SourceCodeGenerator;
use ruff::source_code_locator::SourceCodeLocator;
use ruff::source_code_style::SourceCodeStyleDetector;
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
    let locator = SourceCodeLocator::new(&contents);
    let stylist = SourceCodeStyleDetector::from_contents(&contents, &locator);
    let mut generator = SourceCodeGenerator::new(
        stylist.indentation(),
        stylist.quote(),
        stylist.line_ending(),
    );
    generator.unparse_suite(&python_ast);
    println!("{}", generator.generate());
    Ok(())
}
