//! Generate CLI help.

use anyhow::Result;

use crate::utils::replace_readme_section;

const HELP_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated cli help. -->";
const HELP_END_PRAGMA: &str = "<!-- End auto-generated cli help. -->";

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
    let output = trim_lines(ruff_cli::help().trim());

    if args.dry_run {
        print!("{output}");
    } else {
        replace_readme_section(
            &format!("```\n{output}\n```\n"),
            HELP_BEGIN_PRAGMA,
            HELP_END_PRAGMA,
        )?;
    }

    Ok(())
}
