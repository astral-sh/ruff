//! Run all code and documentation generation steps.

use anyhow::Result;

use crate::{generate_cli_help, generate_docs, generate_json_schema};

pub const REGENERATE_ALL_COMMAND: &str = "cargo dev generate-all";

#[derive(clap::Args)]
pub struct Args {
    #[arg(long)]
    mode: Mode,
}

#[derive(Copy, Clone, PartialEq, Eq, clap::ValueEnum, Default)]
pub enum Mode {
    /// Update the content in the `configuration.md`
    #[default]
    Write,

    /// Don't write to the file, check if the file is up-to-date and error if not
    Check,

    /// Write the generated help to stdout (rather than to `docs/configuration.md`).
    DryRun,
}

impl Mode {
    const fn is_check(self) -> bool {
        matches!(self, Mode::Check)
    }

    pub(crate) const fn is_dry_run(self) -> bool {
        matches!(self, Mode::DryRun)
    }
}

pub fn main(args: &Args) -> Result<()> {
    // Not checked in
    if !args.mode.is_check() {
        generate_docs::main(&generate_docs::Args { dry_run: true })?;
    }
    generate_json_schema::main(&generate_json_schema::Args { mode: args.mode })?;
    generate_cli_help::main(&generate_cli_help::Args { mode: args.mode })?;
    Ok(())
}
