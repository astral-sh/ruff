//! Print the token stream for a given Python file.

use std::path::PathBuf;

use anyhow::Result;

use ruff_linter::linter::parse_unchecked_source;
use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::{PySourceType, PythonVersion, SourceType};

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Python file for which to generate the AST.
    #[arg(required = true)]
    file: PathBuf,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let source_type = PySourceType::from(&args.file);
    let source_kind = SourceKind::from_path(&args.file, SourceType::Python(source_type))?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Could not determine source kind for file: {}",
                args.file.display()
            )
        })?;
    let parsed = parse_unchecked_source(&source_kind, source_type, PythonVersion::default());
    for token in parsed.tokens() {
        println!("{token:#?}");
    }
    Ok(())
}
