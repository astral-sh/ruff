//! Print the AST for a given Python file.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use ruff_python_parser::{parse, Mode};

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Python file for which to generate the AST.
    #[arg(required = true)]
    file: PathBuf,
    /// Run in Jupyter mode i.e., allow line magics.
    #[arg(long)]
    jupyter: bool,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let contents = fs::read_to_string(&args.file)?;
    let mode = if args.jupyter {
        Mode::Jupyter
    } else {
        Mode::Module
    };
    let python_ast = parse(&contents, mode, &args.file.to_string_lossy())?;
    println!("{python_ast:#?}");
    Ok(())
}
