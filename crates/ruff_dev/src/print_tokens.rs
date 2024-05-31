//! Print the token stream for a given Python file.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::Result;

use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::PySourceType;
use ruff_python_parser::parse_unchecked_source;
use ruff_text_size::Ranged;

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
    let parsed = parse_unchecked_source(source_kind.source_code(), source_type);
    for token in parsed.tokens() {
        println!(
            "{start:#?} {kind:#?} {end:#?}",
            start = token.start(),
            end = token.end(),
            kind = token.kind(),
        );
    }
    Ok(())
}
