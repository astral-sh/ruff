#![allow(clippy::print_stdout)]

use std::path::{Path, PathBuf};

use anyhow::{format_err, Context, Result};
use clap::{command, Parser, ValueEnum};

use ruff_formatter::SourceCode;
use ruff_python_ast::PySourceType;
use ruff_python_index::tokens_and_ranges;
use ruff_python_parser::{parse_ok_tokens, AsMode};
use ruff_text_size::Ranged;

use crate::comments::collect_comments;
use crate::{format_module_ast, MagicTrailingComma, PreviewMode, PyFormatOptions};

#[derive(ValueEnum, Clone, Debug)]
pub enum Emit {
    /// Write back to the original files
    Files,
    /// Write to stdout
    Stdout,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[allow(clippy::struct_excessive_bools)] // It's only the dev cli anyways
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
    pub preview: bool,
    #[clap(long)]
    pub print_ir: bool,
    #[clap(long)]
    pub print_comments: bool,
    #[clap(long, short = 'C')]
    pub skip_magic_trailing_comma: bool,
}

pub fn format_and_debug_print(source: &str, cli: &Cli, source_path: &Path) -> Result<String> {
    let source_type = PySourceType::from(source_path);
    let (tokens, comment_ranges) = tokens_and_ranges(source, source_type)
        .map_err(|err| format_err!("Source contains syntax errors {err:?}"))?;

    // Parse the AST.
    let module = parse_ok_tokens(tokens, source, source_type.as_mode(), "<filename>")
        .context("Syntax error in input")?;

    let options = PyFormatOptions::from_extension(source_path)
        .with_preview(if cli.preview {
            PreviewMode::Enabled
        } else {
            PreviewMode::Disabled
        })
        .with_magic_trailing_comma(if cli.skip_magic_trailing_comma {
            MagicTrailingComma::Ignore
        } else {
            MagicTrailingComma::Respect
        });

    let source_code = SourceCode::new(source);
    let formatted = format_module_ast(&module, &comment_ranges, source, options)
        .context("Failed to format node")?;
    if cli.print_ir {
        println!("{}", formatted.document().display(source_code));
    }
    if cli.print_comments {
        // Print preceding, following and enclosing nodes
        let decorated_comments = collect_comments(&module, source_code, &comment_ranges);
        if !decorated_comments.is_empty() {
            println!("# Comment decoration: Range, Preceding, Following, Enclosing, Comment");
        }
        for comment in decorated_comments {
            println!(
                "{:?}, {:?}, {:?}, {:?}, {:?}",
                comment.slice().range(),
                comment
                    .preceding_node()
                    .map(|node| (node.kind(), node.range())),
                comment
                    .following_node()
                    .map(|node| (node.kind(), node.range())),
                (
                    comment.enclosing_node().kind(),
                    comment.enclosing_node().range()
                ),
                comment.slice().text(source_code),
            );
        }
        println!("{:#?}", formatted.context().comments().debug(source_code));
    }
    Ok(formatted
        .print()
        .context("Failed to print the formatter IR")?
        .as_code()
        .to_string())
}
