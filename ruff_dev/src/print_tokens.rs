//! Print the token stream for a given Python file.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use rustpython_parser::lexer;

#[derive(Args)]
pub struct Cli {
    /// Python file for which to generate the AST.
    #[arg(required = true)]
    file: PathBuf,
}

pub fn main(cli: &Cli) -> Result<()> {
    let contents = fs::read_to_string(&cli.file)?;
    for (_, tok, _) in lexer::make_tokenizer(&contents).flatten() {
        println!("{tok:#?}");
    }
    Ok(())
}
