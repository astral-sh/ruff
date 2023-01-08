//! Generate a Markdown-compatible table of supported lint rules.

use anyhow::Result;
use clap::Args;
use itertools::Itertools;
use ruff::registry::{RuleCode, RuleOrigin};
use strum::IntoEnumIterator;

use crate::utils::replace_readme_section;

const TABLE_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated sections. -->";
const TABLE_END_PRAGMA: &str = "<!-- End auto-generated sections. -->";

const TOC_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated table of contents. -->";
const TOC_END_PRAGMA: &str = "<!-- End auto-generated table of contents. -->";

#[derive(Args)]
pub struct Cli {
    /// Write the generated table to stdout (rather than to `README.md`).
    #[arg(long)]
    pub(crate) dry_run: bool,
}

pub fn main(cli: &Cli) -> Result<()> {
    // Generate the table string.
    let mut table_out = String::new();
    let mut toc_out = String::new();
    for origin in RuleOrigin::iter() {
        let codes_csv: String = origin.codes().iter().map(AsRef::as_ref).join(", ");
        table_out.push_str(&format!("### {} ({codes_csv})", origin.title()));
        table_out.push('\n');
        table_out.push('\n');

        toc_out.push_str(&format!(
            "   1. [{} ({})](#{}-{})\n",
            origin.title(),
            codes_csv,
            origin.title().to_lowercase().replace(' ', "-"),
            codes_csv.to_lowercase().replace(',', "-").replace(' ', "")
        ));

        if let Some((url, platform)) = origin.url() {
            table_out.push_str(&format!(
                "For more, see [{}]({}) on {}.",
                origin.title(),
                url,
                platform
            ));
            table_out.push('\n');
            table_out.push('\n');
        }

        table_out.push_str("| Code | Name | Message | Fix |");
        table_out.push('\n');
        table_out.push_str("| ---- | ---- | ------- | --- |");
        table_out.push('\n');

        for rule_code in RuleCode::iter() {
            if rule_code.origin() == origin {
                let kind = rule_code.kind();
                let fix_token = if kind.fixable() { "🛠" } else { "" };
                table_out.push_str(&format!(
                    "| {} | {} | {} | {} |",
                    kind.code().as_ref(),
                    kind.as_ref(),
                    kind.summary().replace('|', r"\|"),
                    fix_token
                ));
                table_out.push('\n');
            }
        }
        table_out.push('\n');
    }

    if cli.dry_run {
        print!("Table of Contents: {toc_out}\n Rules Tables: {table_out}");
    } else {
        // Extra newline in the markdown numbered list looks weird
        replace_readme_section(toc_out.trim_end(), TOC_BEGIN_PRAGMA, TOC_END_PRAGMA)?;
        replace_readme_section(&table_out, TABLE_BEGIN_PRAGMA, TABLE_END_PRAGMA)?;
    }

    Ok(())
}
