//! Generate a Markdown-compatible table of supported lint rules.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::Result;
use itertools::Itertools;
use ruff::registry::{Linter, Rule, RuleNamespace, UpstreamCategory};
use strum::IntoEnumIterator;

use crate::utils::replace_readme_section;

const TABLE_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated sections. -->\n";
const TABLE_END_PRAGMA: &str = "<!-- End auto-generated sections. -->";

const TOC_BEGIN_PRAGMA: &str = "<!-- Begin auto-generated table of contents. -->";
const TOC_END_PRAGMA: &str = "<!-- End auto-generated table of contents. -->";

#[derive(clap::Args)]
pub struct Args {
    /// Write the generated table to stdout (rather than to `README.md`).
    #[arg(long)]
    pub(crate) dry_run: bool,
}

fn generate_table(table_out: &mut String, rules: impl IntoIterator<Item = Rule>) {
    table_out.push_str("| Code | Name | Message | Fix |");
    table_out.push('\n');
    table_out.push_str("| ---- | ---- | ------- | --- |");
    table_out.push('\n');
    for rule in rules {
        let fix_token = match rule.autofixable() {
            None => "",
            Some(_) => "🛠",
        };

        table_out.push_str(&format!(
            "| {} | {} | {} | {} |",
            rule.code(),
            rule.as_ref(),
            rule.message_formats()[0].replace('|', r"\|"),
            fix_token
        ));
        table_out.push('\n');
    }
    table_out.push('\n');
}

pub fn main(args: &Args) -> Result<()> {
    // Generate the table string.
    let mut table_out = String::new();
    let mut toc_out = String::new();
    for linter in Linter::iter() {
        let codes_csv: String = match linter.common_prefix() {
            "" => linter
                .upstream_categories()
                .unwrap()
                .iter()
                .map(|UpstreamCategory(prefix, ..)| prefix.as_ref())
                .join(", "),
            prefix => prefix.to_string(),
        };
        table_out.push_str(&format!("### {} ({codes_csv})", linter.name()));
        table_out.push('\n');
        table_out.push('\n');

        toc_out.push_str(&format!(
            "   1. [{} ({})](#{}-{})\n",
            linter.name(),
            codes_csv,
            linter.name().to_lowercase().replace(' ', "-"),
            codes_csv.to_lowercase().replace(',', "-").replace(' ', "")
        ));

        if let Some(url) = linter.url() {
            let host = url
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap();
            table_out.push_str(&format!(
                "For more, see [{}]({}) on {}.",
                linter.name(),
                url,
                match host {
                    "pypi.org" => "PyPI",
                    "github.com" => "GitHub",
                    host => panic!(
                        "unexpected host in URL of {}, expected pypi.org or github.com but found \
                         {host}",
                        linter.name()
                    ),
                }
            ));
            table_out.push('\n');
            table_out.push('\n');
        }

        if let Some(categories) = linter.upstream_categories() {
            for UpstreamCategory(prefix, name) in categories {
                table_out.push_str(&format!("#### {name} ({})", prefix.as_ref()));
                table_out.push('\n');
                table_out.push('\n');
                generate_table(&mut table_out, prefix);
            }
        } else {
            generate_table(&mut table_out, &linter);
        }
    }

    if args.dry_run {
        print!("Table of Contents: {toc_out}\n Rules Tables: {table_out}");
    } else {
        // Extra newline in the markdown numbered list looks weird
        replace_readme_section(toc_out.trim_end(), TOC_BEGIN_PRAGMA, TOC_END_PRAGMA)?;
        replace_readme_section(&table_out, TABLE_BEGIN_PRAGMA, TABLE_END_PRAGMA)?;
    }

    Ok(())
}
