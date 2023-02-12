//! Generate Markdown documentation for applicable rules.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;

use anyhow::Result;
use ruff::registry::{Linter, Rule, RuleNamespace};
use ruff::AutofixAvailability;
use strum::IntoEnumIterator;

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated docs to stdout (rather than to the filesystem).
    #[arg(long)]
    pub(crate) dry_run: bool,
}

pub fn main(args: &Args) -> Result<()> {
    for rule in Rule::iter() {
        if let Some(explanation) = rule.explanation() {
            let mut output = String::new();
            output.push_str(&format!("# {} ({})", rule.as_ref(), rule.code()));
            output.push('\n');
            output.push('\n');

            let (linter, _) = Linter::parse_code(rule.code()).unwrap();
            output.push_str(&format!("Derived from the **{}** linter.", linter.name()));
            output.push('\n');
            output.push('\n');

            if let Some(autofix) = rule.autofixable() {
                output.push_str(match autofix.available {
                    AutofixAvailability::Sometimes => "Autofix is sometimes available.",
                    AutofixAvailability::Always => "Autofix is always available.",
                });
                output.push('\n');
                output.push('\n');
            }

            output.push_str(explanation.trim());

            if args.dry_run {
                println!("{output}");
            } else {
                fs::create_dir_all("docs/rules")?;
                fs::write(format!("docs/rules/{}.md", rule.as_ref()), output)?;
            }
        }
    }
    Ok(())
}
