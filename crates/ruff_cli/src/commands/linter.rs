use itertools::Itertools;
use ruff::registry::{Linter, RuleNamespace, UpstreamCategory};
use serde::Serialize;
use strum::IntoEnumIterator;

use crate::args::HelpFormat;

pub fn linter(format: HelpFormat) {
    match format {
        HelpFormat::Text => {
            for linter in Linter::iter() {
                let prefix = match linter.common_prefix() {
                    "" => linter
                        .upstream_categories()
                        .unwrap()
                        .iter()
                        .map(|UpstreamCategory(prefix, ..)| prefix.as_ref())
                        .join("/"),
                    prefix => prefix.to_string(),
                };

                #[allow(clippy::print_stdout)]
                {
                    println!("{:>4} {}", prefix, linter.name());
                }
            }
        }

        HelpFormat::Json => {
            let linters: Vec<_> = Linter::iter()
                .map(|linter_info| LinterInfo {
                    prefix: linter_info.common_prefix(),
                    name: linter_info.name(),
                    categories: linter_info.upstream_categories().map(|cats| {
                        cats.iter()
                            .map(|UpstreamCategory(prefix, name, ..)| LinterCategoryInfo {
                                prefix: prefix.as_ref(),
                                name,
                            })
                            .collect()
                    }),
                })
                .collect();

            #[allow(clippy::print_stdout)]
            {
                println!("{}", serde_json::to_string_pretty(&linters).unwrap());
            }
        }
    }
}

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
