/// Print the AST for a given Python file.
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use ruff::fs;

#[derive(Debug, Parser)]
struct Cli {
    #[arg(required = true)]
    file: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let contents = fs::read_file(&cli.file)?;
    let ast = python_parser::file_input(python_parser::make_strspan(&contents))?.1;
    println!("{:?}", ast);

    Ok(())
}
