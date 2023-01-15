//! Print the token stream for a given Python file.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use rustpython_ast::Location;
use rustpython_parser::lexer;
use rustpython_parser::lexer::{LexResult, Tok};

#[derive(Args)]
pub struct Cli {
    /// Python file for which to generate the AST.
    #[arg(required = true)]
    file: PathBuf,
}

pub fn lines(lxr: &[LexResult]) -> Vec<usize> {
    let mut continuation_lines = Vec::new();
    let mut prev: Option<(&Location, &Tok, &Location)> = None;
    for (start, tok, end) in lxr.iter().flatten() {
        if let Some((.., prev_tok, prev_end)) = prev {
            if !matches!(
                prev_tok,
                Tok::Newline | Tok::NonLogicalNewline | Tok::Comment(..)
            ) {
                for line in prev_end.row()..start.row() {
                    continuation_lines.push(line);
                }
            }
        }
        prev = Some((start, tok, end));
    }
    continuation_lines
}

pub fn main(cli: &Cli) -> Result<()> {
    let contents = fs::read_to_string(&cli.file)?;
    // for (start, tok, end) in lexer::make_tokenizer(&contents).flatten() {
    //     println!("{tok:#?} ({start:?}, {end:?})");
    // }
    let lxr: Vec<LexResult> = lexer::make_tokenizer(&contents).collect();
    println!("{:?}", lines(&lxr));
    Ok(())
}
