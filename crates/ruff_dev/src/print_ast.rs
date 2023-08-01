//! Print the AST for a given Python file.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use ruff_python_parser::parse_suite;

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Python file for which to generate the AST.
    #[arg(required = true)]
    file: PathBuf,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let contents = fs::read_to_string(&args.file)?;
    let python_ast = parse_suite(&contents, &args.file.to_string_lossy())?;
    println!("{python_ast:#?}");
    Ok(())
}
