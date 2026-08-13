//! Generate a Markdown-compatible listing of Ruff's default lint rules.

use std::fmt::Write;

use itertools::Itertools;
use strum::IntoEnumIterator;

use ruff_linter::registry::{Linter, RuleNamespace};
use ruff_linter::settings::LinterSettings;

pub(crate) fn generate() -> String {
    let default_rules = LinterSettings::default().rules;
    let linters = Linter::iter()
        .filter_map(|linter| {
            let rules = linter
                .all_rules()
                .filter(|rule| default_rules.enabled(*rule))
                .collect_vec();
            (!rules.is_empty()).then_some((linter, rules))
        })
        .collect_vec();

    let mut output = String::new();
    output.push_str("# Default Rules\n\n");
    output.push_str("Ruff enables the following rules by default:\n\n");

    output.push_str("??? note \"Default `select` configuration\"\n\n");
    for (filename, section) in [
        ("pyproject.toml", "[tool.ruff.lint]"),
        ("ruff.toml", "[lint]"),
    ] {
        let _ = writeln!(output, "    === \"{filename}\"\n");
        output.push_str("        ```toml\n");
        let _ = writeln!(output, "        {section}");
        output.push_str("        select = [\n");
        for (_, rules) in &linters {
            for rule in rules {
                let _ = writeln!(output, "            \"{}\",", rule.noqa_code());
            }
        }
        output.push_str("        ]\n");
        output.push_str("        ```\n\n");
    }

    for (linter, rules) in &linters {
        let codes = match linter.common_prefix() {
            "" => linter
                .upstream_categories()
                .unwrap()
                .iter()
                .map(|category| category.prefix)
                .join(", "),
            prefix => prefix.to_string(),
        };
        let _ = writeln!(output, "## {} ({codes})\n", linter.name());

        for rule in rules {
            let name = rule.name();
            let code = rule.noqa_code();
            let _ = writeln!(output, "- [`{name}`](rules/{name}.md) (`{code}`)");
        }
        output.push('\n');
    }

    output
}
