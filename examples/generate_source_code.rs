use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use rustpython_parser::parser;

use ruff::code_gen::SourceGenerator;
use ruff::fs;

#[derive(Debug, Parser)]
struct Cli {
    #[arg(required = true)]
    file: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let contents = fs::read_file(&cli.file)?;
    let python_ast = parser::parse_program(&contents, &cli.file.to_string_lossy())?;
    let mut generator = SourceGenerator::new();
    generator.unparse_suite(&python_ast)?;
    println!("{}", generator.generate()?);

    Ok(())
}
