//! Generate CLI help.

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, CommandFactory};
use ruff::cli::Cli as MainCli;

const HELP_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated cli help. -->";
const HELP_END_PRAGMA: &str = "<!-- End auto-generated cli help. -->";

#[derive(Args)]
pub struct Cli {
    /// Write the generated help to stdout (rather than to `README.md`).
    #[arg(long)]
    pub(crate) dry_run: bool,
}

pub fn main(cli: &Cli) -> Result<()> {
    let mut cmd = MainCli::command();
    let output = cmd.render_help().to_string();

    if cli.dry_run {
        print!("{output}");
    } else {
        replace_readme_section(
            &format!("```shell\n{output}\n```\n"),
            HELP_BEGIN_PRAGMA,
            HELP_END_PRAGMA,
        )?;
    }

    Ok(())
}

fn replace_readme_section(content: &str, begin_pragma: &str, end_pragma: &str) -> Result<()> {
    // Read the existing file.
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to find root directory")
        .join("README.md");
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
