//! Print the token stream for a given Python file.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use rustpython_parser::{lexer, Mode};

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Python file for which to generate the AST.
    #[arg(required = true)]
    file: PathBuf,
    /// Run in Jupyter mode i.e., allow line magics (`%`, `!`, `?`, `/`, `,`, `;`).
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
    for (tok, range) in lexer::lex(&contents, mode).flatten() {
        println!(
            "{start:#?} {tok:#?} {end:#?}",
            start = range.start(),
            end = range.end()
        );
    }
    Ok(())
}
