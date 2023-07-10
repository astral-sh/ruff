#![allow(clippy::print_stdout)]

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{command, Parser, ValueEnum};
use rustpython_parser::lexer::lex;
use rustpython_parser::{parse_tokens, Mode};

use ruff_formatter::SourceCode;
use ruff_python_ast::source_code::CommentRangesBuilder;

use crate::{format_node, PyFormatOptions};

#[derive(ValueEnum, Clone, Debug)]
pub enum Emit {
    /// Write back to the original files
    Files,
    /// Write to stdout
    Stdout,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Python files to format. If there are none, stdin will be used. `-` as stdin is not supported
    pub files: Vec<PathBuf>,
    #[clap(long)]
    pub emit: Option<Emit>,
    /// Run in 'check' mode. Exits with 0 if input is formatted correctly. Exits with 1 and prints
    /// a diff if formatting is required.
    #[clap(long)]
    pub check: bool,
    #[clap(long)]
    pub print_ir: bool,
    #[clap(long)]
    pub print_comments: bool,
}

pub fn format_and_debug_print(input: &str, cli: &Cli) -> Result<String> {
    let mut tokens = Vec::new();
    let mut comment_ranges = CommentRangesBuilder::default();

    for result in lex(input, Mode::Module) {
        let (token, range) = match result {
            Ok((token, range)) => (token, range),
            Err(err) => bail!("Source contains syntax errors {err:?}"),
        };

        comment_ranges.visit_token(&token, range);
        tokens.push(Ok((token, range)));
    }

    let comment_ranges = comment_ranges.finish();

    // Parse the AST.
    let python_ast = parse_tokens(tokens, Mode::Module, "<filename>")
        .with_context(|| "Syntax error in input")?;

    let formatted = format_node(
        &python_ast,
        &comment_ranges,
        input,
        PyFormatOptions::default(),
    )?;
    if cli.print_ir {
        println!("{}", formatted.document().display(SourceCode::new(input)));
    }
    if cli.print_comments {
        println!(
            "{:#?}",
            formatted.context().comments().debug(SourceCode::new(input))
        );
    }
    Ok(formatted
        .print()
        .with_context(|| "Failed to print the formatter IR")?
        .as_code()
        .to_string())
}
