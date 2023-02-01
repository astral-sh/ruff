use itertools::Itertools;
use serde::Serialize;
use strum::IntoEnumIterator;

use ruff::registry::{Linter, LinterCategory, RuleNamespace};

use crate::args::HelpFormat;

pub fn linter(format: HelpFormat) {
    match format {
        HelpFormat::Text => {
            for linter in Linter::iter() {
                let prefix = match linter.common_prefix() {
                    "" => linter
                        .categories()
                        .unwrap()
                        .iter()
                        .map(|LinterCategory(prefix, ..)| prefix)
                        .join("/"),
                    prefix => prefix.to_string(),
                };
                println!("{:>4} {}", prefix, linter.name());
            }
        }

        HelpFormat::Json => {
            let linters: Vec<_> = Linter::iter()
                .map(|linter_info| LinterInfo {
                    prefix: linter_info.common_prefix(),
                    name: linter_info.name(),
                    categories: linter_info.categories().map(|cats| {
                        cats.iter()
                            .map(|LinterCategory(prefix, name, ..)| LinterCategoryInfo {
                                prefix,
                                name,
                            })
                            .collect()
                    }),
                })
                .collect();

            println!("{}", serde_json::to_string_pretty(&linters).unwrap());
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
