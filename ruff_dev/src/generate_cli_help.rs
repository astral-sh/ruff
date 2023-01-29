//! Generate CLI help.

use crate::utils::replace_readme_section;
use anyhow::Result;
use assert_cmd::Command;
use std::str;
const BIN_NAME: &str = "ruff";

const COMMAND_HELP_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated command help. -->";
const COMMAND_HELP_END_PRAGMA: &str = "<!-- End auto-generated command help. -->";

const SUBCOMMAND_HELP_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated subcommand help. -->";
const SUBCOMMAND_HELP_END_PRAGMA: &str = "<!-- End auto-generated subcommand help. -->";

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated help to stdout (rather than to `README.md`).
    #[arg(long)]
    pub(crate) dry_run: bool,
}

pub fn main(args: &Args) -> Result<()> {
    // Generate `ruff help`.
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let output = cmd.args(["help"]).assert().success();
    let command_help = str::from_utf8(&output.get_output().stdout)?.trim();

    // Generate `ruff help check`.
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let output = cmd.args(["help", "check"]).assert().success();
    let subcommand_help = str::from_utf8(&output.get_output().stdout)?.trim();

    if args.dry_run {
        print!("{command_help}");
        print!("{subcommand_help}");
    } else {
        replace_readme_section(
            &format!("```\n{command_help}\n```\n"),
            COMMAND_HELP_BEGIN_PRAGMA,
            COMMAND_HELP_END_PRAGMA,
        )?;
        replace_readme_section(
            &format!("```\n{subcommand_help}\n```\n"),
            SUBCOMMAND_HELP_BEGIN_PRAGMA,
            SUBCOMMAND_HELP_END_PRAGMA,
        )?;
    }

    Ok(())
}
