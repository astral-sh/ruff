//! Generate a Markdown-compatible table of supported lint rules.

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;

use anyhow::Result;
use clap::Args;
use ruff::checks::{CheckCategory, CheckCode};
use strum::IntoEnumIterator;

const FILE: &str = "../README.md";
const BEGIN_PRAGMA: &str = "<!-- Begin auto-generated sections. -->";
const END_PRAGMA: &str = "<!-- End auto-generated sections. -->";

#[derive(Args)]
pub struct Cli {
    /// Write the generated table to stdout (rather than to `README.md`).
    #[arg(long)]
    dry_run: bool,
}

pub fn main(cli: &Cli) -> Result<()> {
    // Generate the table string.
    let mut output = String::new();
    for check_category in CheckCategory::iter() {
        output.push_str(&format!("### {}", check_category.title()));
        output.push('\n');
        output.push('\n');

        if let Some(url) = check_category.url() {
            output.push_str(&format!(
                "For more, see [{}]({}) on PyPI.",
                check_category.title(),
                url
            ));
            output.push('\n');
            output.push('\n');
        }

        output.push_str("| Code | Name | Message | Fix |");
        output.push('\n');
        output.push_str("| ---- | ---- | ------- | --- |");
        output.push('\n');

        for check_code in CheckCode::iter() {
            if check_code.category() == check_category {
                let check_kind = check_code.kind();
                let fix_token = if check_kind.fixable() { "🛠" } else { "" };
                output.push_str(&format!(
                    "| {} | {} | {} | {} |",
                    check_kind.code().as_ref(),
                    check_kind.as_ref(),
                    check_kind.summary().replace('|', r"\|"),
                    fix_token
                ));
                output.push('\n');
            }
        }
        output.push('\n');
    }

    if cli.dry_run {
        print!("{}", output);
    } else {
        // Read the existing file.
        let existing = fs::read_to_string(FILE)?;

        // Extract the prefix.
        let index = existing
            .find(BEGIN_PRAGMA)
            .expect("Unable to find begin pragma.");
        let prefix = &existing[..index + BEGIN_PRAGMA.len()];

        // Extract the suffix.
        let index = existing
            .find(END_PRAGMA)
            .expect("Unable to find end pragma.");
        let suffix = &existing[index..];

        // Write the prefix, new contents, and suffix.
        let mut f = OpenOptions::new().write(true).truncate(true).open(FILE)?;
        write!(f, "{}\n\n", prefix)?;
        write!(f, "{}", output)?;
        write!(f, "{}", suffix)?;
    }

    Ok(())
}
