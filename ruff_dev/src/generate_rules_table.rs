//! Generate a Markdown-compatible table of supported lint rules.

use anyhow::Result;
use clap::Args;
use itertools::Itertools;
use ruff::registry::{CheckCategory, DiagnosticCode};
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
    for check_category in CheckCategory::iter() {
        let codes_csv: String = check_category.codes().iter().map(AsRef::as_ref).join(", ");
        table_out.push_str(&format!("### {} ({codes_csv})", check_category.title()));
        table_out.push('\n');
        table_out.push('\n');

        toc_out.push_str(&format!(
            "   1. [{} ({})](#{}-{})\n",
            check_category.title(),
            codes_csv,
            check_category.title().to_lowercase().replace(' ', "-"),
            codes_csv.to_lowercase().replace(',', "-").replace(' ', "")
        ));

        if let Some((url, platform)) = check_category.url() {
            table_out.push_str(&format!(
                "For more, see [{}]({}) on {}.",
                check_category.title(),
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

        for check_code in DiagnosticCode::iter() {
            if check_code.category() == check_category {
                let check_kind = check_code.kind();
                let fix_token = if check_kind.fixable() { "🛠" } else { "" };
                table_out.push_str(&format!(
                    "| {} | {} | {} | {} |",
                    check_kind.code().as_ref(),
                    check_kind.as_ref(),
                    check_kind.summary().replace('|', r"\|"),
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
