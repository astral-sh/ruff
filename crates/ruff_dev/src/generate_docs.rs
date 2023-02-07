//! Generate Markdown documentation for applicable rules.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;

use anyhow::Result;
use strum::IntoEnumIterator;

use ruff::registry::Rule;

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated docs to stdout (rather than to the filesystem).
    #[arg(long)]
    pub(crate) dry_run: bool,
}

pub fn main(args: &Args) -> Result<()> {
    for rule in Rule::iter() {
        if let Some(explanation) = rule.explanation() {
            let explanation = format!("# {} ({})\n\n{}", rule.as_ref(), rule.code(), explanation);

            if args.dry_run {
                println!("{}", explanation);
            } else {
                fs::create_dir_all("docs/rules")?;
                fs::write(format!("docs/rules/{}.md", rule.as_ref()), explanation)?;
            }
        }
    }
    Ok(())
}
