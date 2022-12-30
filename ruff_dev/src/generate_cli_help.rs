//! Generate CLI help.

use anyhow::Result;
use clap::{Args, CommandFactory};
use ruff::cli::Cli as MainCli;

use crate::utils::replace_readme_section;

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
