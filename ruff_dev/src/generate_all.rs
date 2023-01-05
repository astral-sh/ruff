//! Run all code and documentation generation steps.

use anyhow::Result;
use clap::Args;

use crate::{generate_cli_help, generate_json_schema, generate_options, generate_rules_table};

#[derive(Args)]
pub struct Cli {
    /// Write the generated artifacts to stdout (rather than to the filesystem).
    #[arg(long)]
    dry_run: bool,
}

pub fn main(cli: &Cli) -> Result<()> {
    generate_json_schema::main(&generate_json_schema::Cli {
        dry_run: cli.dry_run,
    })?;
    generate_rules_table::main(&generate_rules_table::Cli {
        dry_run: cli.dry_run,
    })?;
    generate_options::main(&generate_options::Cli {
        dry_run: cli.dry_run,
    })?;
    generate_cli_help::main(&generate_cli_help::Cli {
        dry_run: cli.dry_run,
    })?;
    Ok(())
}
