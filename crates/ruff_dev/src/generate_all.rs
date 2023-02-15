//! Run all code and documentation generation steps.

use anyhow::Result;

use crate::{generate_cli_help, generate_docs, generate_json_schema, generate_options};

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated artifacts to stdout (rather than to the filesystem).
    #[arg(long)]
    dry_run: bool,
}

pub fn main(args: &Args) -> Result<()> {
    generate_docs::main(&generate_docs::Args {
        dry_run: args.dry_run,
    })?;
    generate_json_schema::main(&generate_json_schema::Args {
        dry_run: args.dry_run,
    })?;
    generate_options::main(&generate_options::Args {
        dry_run: args.dry_run,
    })?;
    generate_cli_help::main(&generate_cli_help::Args {
        dry_run: args.dry_run,
    })?;
    Ok(())
}
