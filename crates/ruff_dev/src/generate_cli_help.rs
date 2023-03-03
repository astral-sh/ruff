//! Generate CLI help.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;
use std::{fs, str};

use anyhow::{bail, Result};
use pretty_assertions::StrComparison;

use crate::generate_all::REGENERATE_ALL_COMMAND;
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
    /// Don't write to the file, check if the file is up-to-date and error if not
    #[arg(long)]
    pub(crate) check: bool,
}

fn trim_lines(s: &str) -> String {
    s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}

/// Takes the existing file contents, inserts the section, returns the transformed content
fn replace_docs_section(
    existing: &str,
    section: &str,
    begin_pragma: &str,
    end_pragma: &str,
) -> String {
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

    format!("{prefix}\n{section}{suffix}")
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
        // Read the existing file.
        let filename = "docs/configuration.md";
        let file = PathBuf::from(ROOT_DIR).join(filename);
        let existing = fs::read_to_string(&file)?;

        let new = replace_docs_section(
            &existing,
            &format!("```text\n{command_help}\n```\n\n"),
            COMMAND_HELP_BEGIN_PRAGMA,
            COMMAND_HELP_END_PRAGMA,
        );
        let new = replace_docs_section(
            &new,
            &format!("```text\n{subcommand_help}\n```\n\n"),
            SUBCOMMAND_HELP_BEGIN_PRAGMA,
            SUBCOMMAND_HELP_END_PRAGMA,
        );

        if args.check {
            if existing == new {
                println!("up-to-date: {filename}");
            } else {
                let comparison = StrComparison::new(&existing, &new);
                bail!("{filename} changed, please run `{REGENERATE_ALL_COMMAND}`:\n{comparison}");
            }
        } else {
            fs::write(file, &new)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::{main, Args};

    #[test]
    fn test_generate_json_schema() {
        main(&Args {
            dry_run: false,
            check: true,
        })
        .unwrap();
    }
}
