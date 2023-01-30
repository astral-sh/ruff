//! Generate Markdown documentation for applicable rules.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;

use anyhow::Result;
use ruff::registry::{Linter, Rule, RuleNamespace};
use ruff::settings::options::Options;
use ruff::settings::options_base::ConfigurationOptions;
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
            output.push_str(&format!("# {} ({})", rule.as_ref(), rule.noqa_code()));
            output.push('\n');
            output.push('\n');

            let (linter, _) = Linter::parse_code(rule.noqa_code()).unwrap();
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

            process_documentation(explanation.trim(), &mut output);

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

fn process_documentation(documentation: &str, out: &mut String) {
    let mut in_options = false;
    let mut after = String::new();

    for line in documentation.split_inclusive('\n') {
        if line.starts_with("## ") {
            in_options = line == "## Options\n";
        } else if in_options {
            if let Some(rest) = line.strip_prefix("* `") {
                let option = rest.trim_end().trim_end_matches('`');

                assert!(
                    Options::get(Some(option)).is_some(),
                    "unknown option {option}"
                );

                let anchor = option.rsplit('.').next().unwrap();
                out.push_str(&format!("* [`{option}`]\n"));
                after.push_str(&format!("[`{option}`]: ../../settings#{anchor}"));

                continue;
            }
        }

        out.push_str(line);
    }
    if !after.is_empty() {
        out.push_str("\n\n");
        out.push_str(&after);
    }
}
