use std::fmt::Write;
use std::io;
use std::io::BufWriter;

use anyhow::Result;
use itertools::Itertools;
use serde::Serialize;
use strum::IntoEnumIterator;

use ruff_linter::registry::{Linter, RuleNamespace};

use crate::args::HelpFormat;

#[derive(Serialize)]
struct LinterInfo {
    prefix: &'static str,
    name: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    categories: Option<Vec<LinterCategoryInfo>>,
}

#[derive(Serialize)]
struct LinterCategoryInfo {
    prefix: &'static str,
    name: &'static str,
}

pub(crate) fn linter(format: HelpFormat) -> Result<()> {
    let mut stdout = BufWriter::new(io::stdout().lock());
    let mut output = String::new();

    match format {
        HelpFormat::Text => {
            for linter in Linter::iter() {
                let prefix = match linter.common_prefix() {
                    "" => linter
                        .upstream_categories()
                        .unwrap()
                        .iter()
                        .map(|c| c.prefix)
                        .join("/"),
                    prefix => prefix.to_string(),
                };
                writeln!(output, "{:>4} {}", prefix, linter.name()).unwrap();
            }
        }

        HelpFormat::Json => {
            let linters: Vec<_> = Linter::iter()
                .map(|linter_info| LinterInfo {
                    prefix: linter_info.common_prefix(),
                    name: linter_info.name(),
                    categories: linter_info.upstream_categories().map(|cats| {
                        cats.iter()
                            .map(|c| LinterCategoryInfo {
                                prefix: c.prefix,
                                name: c.category,
                            })
                            .collect()
                    }),
                })
                .collect();
            output.push_str(&serde_json::to_string_pretty(&linters)?);
            output.push('\n');
        }
    }

    io::Write::write_fmt(&mut stdout, format_args!("{output}"))?;

    Ok(())
}
