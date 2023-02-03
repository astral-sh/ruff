//! Generate CLI help.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use crate::utils::replace_readme_section;
use anyhow::Result;
use std::str;

const COMMAND_HELP_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated command help. -->\n";
const COMMAND_HELP_END_PRAGMA: &str = "<!-- End auto-generated command help. -->";

const SUBCOMMAND_HELP_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated subcommand help. -->\n";
const SUBCOMMAND_HELP_END_PRAGMA: &str = "<!-- End auto-generated subcommand help. -->";

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated help to stdout (rather than to `README.md`).
    #[arg(long)]
    pub(crate) dry_run: bool,
}

fn trim_lines(s: &str) -> String {
    s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}

pub fn main(args: &Args) -> Result<()> {
    // Generate `ruff help`.
    let command_help = trim_lines(ruff_cli::command_help().trim());

    // Generate `ruff help check`.
    let subcommand_help = trim_lines(ruff_cli::subcommand_help().trim());

    if args.dry_run {
        print!("{command_help}");
        print!("{subcommand_help}");
    } else {
        replace_readme_section(
            &format!("```text\n{command_help}\n```\n\n"),
            COMMAND_HELP_BEGIN_PRAGMA,
            COMMAND_HELP_END_PRAGMA,
        )?;
        replace_readme_section(
            &format!("```text\n{subcommand_help}\n```\n\n"),
            SUBCOMMAND_HELP_BEGIN_PRAGMA,
            SUBCOMMAND_HELP_END_PRAGMA,
        )?;
    }

    Ok(())
}
