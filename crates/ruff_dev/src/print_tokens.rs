//! Print the token stream for a given Python file.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::Result;

use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::PySourceType;
use ruff_python_parser::{lexer, AsMode};

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Python file for which to generate the AST.
    #[arg(required = true)]
    file: PathBuf,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let source_type = PySourceType::from(&args.file);
    let source_kind = SourceKind::from_path(&args.file, source_type)?.ok_or_else(|| {
        anyhow::anyhow!(
            "Could not determine source kind for file: {}",
            args.file.display()
        )
    })?;
    for (tok, range) in lexer::lex(source_kind.source_code(), source_type.as_mode()).flatten() {
        println!(
            "{start:#?} {tok:#?} {end:#?}",
            start = range.start(),
            end = range.end()
        );
    }
    Ok(())
}
