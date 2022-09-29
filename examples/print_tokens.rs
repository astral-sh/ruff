/// Print the token stream for a given Python file.
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use rustpython_parser::lexer;

use ruff::fs;

#[derive(Debug, Parser)]
struct Cli {
    #[arg(required = true)]
    file: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let contents = fs::read_file(&cli.file)?;
    for (_, tok, _) in lexer::make_tokenizer(&contents).flatten() {
        println!("{:#?}", tok);
    }

    Ok(())
}
