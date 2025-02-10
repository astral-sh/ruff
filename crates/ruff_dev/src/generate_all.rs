//! Run all code and documentation generation steps.

use anyhow::Result;

use crate::{generate_cli_help, generate_docs, generate_json_schema, generate_knot_schema};

pub(crate) const REGENERATE_ALL_COMMAND: &str = "cargo dev generate-all";

#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long, default_value_t, value_enum)]
    mode: Mode,
}

#[derive(Copy, Clone, PartialEq, Eq, clap::ValueEnum, Default)]
pub(crate) enum Mode {
    /// Update the content in the `configuration.md`.
    #[default]
    Write,

    /// Don't write to the file, check if the file is up-to-date and error if not.
    Check,

    /// Write the generated help to stdout.
    DryRun,
}

impl Mode {
    pub(crate) const fn is_dry_run(self) -> bool {
        matches!(self, Mode::DryRun)
    }
}

pub(crate) fn main(args: &Args) -> Result<()> {
    generate_json_schema::main(&generate_json_schema::Args { mode: args.mode })?;
    generate_knot_schema::main(&generate_knot_schema::Args { mode: args.mode })?;
    generate_cli_help::main(&generate_cli_help::Args { mode: args.mode })?;
    generate_docs::main(&generate_docs::Args {
        dry_run: args.mode.is_dry_run(),
    })?;
    Ok(())
}
