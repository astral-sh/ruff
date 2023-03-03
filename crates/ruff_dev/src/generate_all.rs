//! Run all code and documentation generation steps.

use anyhow::Result;

use crate::{generate_cli_help, generate_docs, generate_json_schema};

pub const REGENERATE_ALL_COMMAND: &str = "cargo dev generate-all";

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated artifacts to stdout (rather than to the filesystem).
    #[arg(long)]
    dry_run: bool,
    /// Don't write to the file, check if the file is up-to-date and error if not
    #[arg(long)]
    check: bool,
}

pub fn main(args: &Args) -> Result<()> {
    generate_docs::main(&generate_docs::Args {
        dry_run: args.dry_run,
        check: args.check,
    })?;
    generate_json_schema::main(&generate_json_schema::Args {
        dry_run: args.dry_run,
        check: args.check,
    })?;
    generate_cli_help::main(&generate_cli_help::Args {
        dry_run: args.dry_run,
        check: args.check,
    })?;
    Ok(())
}
