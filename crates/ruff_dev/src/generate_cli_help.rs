//! Generate CLI help.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::{fs, str};

use anyhow::Result;

use crate::ROOT_DIR;

const COMMAND_HELP_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated command help. -->\n";
const COMMAND_HELP_END_PRAGMA: &str = "<!-- End auto-generated command help. -->";

const SUBCOMMAND_HELP_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated subcommand help. -->\n";
const SUBCOMMAND_HELP_END_PRAGMA: &str = "<!-- End auto-generated subcommand help. -->";

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated help to stdout (rather than to `docs/configuration.md`).
    #[arg(long)]
    pub(crate) dry_run: bool,
}

fn trim_lines(s: &str) -> String {
    s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}

fn replace_docs_section(content: &str, begin_pragma: &str, end_pragma: &str) -> Result<()> {
    // Read the existing file.
    let file = PathBuf::from(ROOT_DIR).join("docs/configuration.md");
    let existing = fs::read_to_string(&file)?;

    // Extract the prefix.
    let index = existing
        .find(begin_pragma)
        .expect("Unable to find begin pragma");
    let prefix = &existing[..index + begin_pragma.len()];

    // Extract the suffix.
    let index = existing
        .find(end_pragma)
        .expect("Unable to find end pragma");
    let suffix = &existing[index..];

    // Write the prefix, new contents, and suffix.
    let mut f = OpenOptions::new().write(true).truncate(true).open(&file)?;
    writeln!(f, "{prefix}")?;
    write!(f, "{content}")?;
    write!(f, "{suffix}")?;

    Ok(())
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
        replace_docs_section(
            &format!("```text\n{command_help}\n```\n\n"),
            COMMAND_HELP_BEGIN_PRAGMA,
            COMMAND_HELP_END_PRAGMA,
        )?;
        replace_docs_section(
            &format!("```text\n{subcommand_help}\n```\n\n"),
            SUBCOMMAND_HELP_BEGIN_PRAGMA,
            SUBCOMMAND_HELP_END_PRAGMA,
        )?;
    }

    Ok(())
}
