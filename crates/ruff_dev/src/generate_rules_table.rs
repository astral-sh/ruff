//! Generate a Markdown-compatible table of supported lint rules.
//!
//! Used for <https://beta.ruff.rs/docs/rules/>

use itertools::Itertools;
use strum::IntoEnumIterator;

use ruff::registry::{Linter, Rule, RuleNamespace};
use ruff::settings::options::Options;
use ruff::upstream_categories::UpstreamCategoryAndPrefix;
use ruff_diagnostics::AutofixKind;

const FIX_SYMBOL: &str = "🛠";
const NURSERY_SYMBOL: &str = "🌅";

fn generate_table(table_out: &mut String, rules: impl IntoIterator<Item = Rule>, linter: &Linter) {
    table_out.push_str("| Code | Name | Message | |");
    table_out.push('\n');
    table_out.push_str("| ---- | ---- | ------- | ------: |");
    table_out.push('\n');
    for rule in rules {
        let fix_token = match rule.autofixable() {
            AutofixKind::Always | AutofixKind::Sometimes => {
                format!("<span style='opacity: 1'>{FIX_SYMBOL}</span>")
            }
            AutofixKind::None => format!("<span style='opacity: 0.1'>{FIX_SYMBOL}</span>"),
        };
        let nursery_token = if rule.is_nursery() {
            format!("<span style='opacity: 1'>{NURSERY_SYMBOL}</span>")
        } else {
            format!("<span style='opacity: 0.1'>{NURSERY_SYMBOL}</span>")
        };
        let status_token = format!("{fix_token} {nursery_token}");

        let rule_name = rule.as_ref();

        #[allow(clippy::or_fun_call)]
        table_out.push_str(&format!(
            "| {0}{1} {{ #{0}{1} }} | {2} | {3} | {4} |",
            linter.common_prefix(),
            linter.code_for_rule(rule).unwrap(),
            rule.explanation()
                .is_some()
                .then_some(format_args!("[{rule_name}](rules/{rule_name}.md)"))
                .unwrap_or(format_args!("{rule_name}")),
            rule.message_formats()[0],
            status_token,
        ));
        table_out.push('\n');
    }
    table_out.push('\n');
}

pub(crate) fn generate() -> String {
    // Generate the table string.
    let mut table_out = String::new();

    table_out.push_str(&format!(
        "The {FIX_SYMBOL} emoji indicates that a rule is automatically fixable by the `--fix` command-line option."));
    table_out.push('\n');
    table_out.push('\n');

    table_out.push_str(&format!(
        "The {NURSERY_SYMBOL} emoji indicates that a rule is part of the [\"nursery\"](../faq/#what-is-the-nursery)."
    ));
    table_out.push('\n');
    table_out.push('\n');

    for linter in Linter::iter() {
        let codes_csv: String = match linter.common_prefix() {
            "" => linter
                .upstream_categories()
                .unwrap()
                .iter()
                .map(|c| c.prefix)
                .join(", "),
            prefix => prefix.to_string(),
        };
        table_out.push_str(&format!("### {} ({codes_csv})", linter.name()));
        table_out.push('\n');
        table_out.push('\n');

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

        if Options::metadata()
            .iter()
            .any(|(name, _)| name == &linter.name())
        {
            table_out.push_str(&format!(
                "For related settings, see [{}](settings.md#{}).",
                linter.name(),
                linter.name(),
            ));
            table_out.push('\n');
            table_out.push('\n');
        }

        let rules_by_upstream_category = linter
            .all_rules()
            .map(|rule| (rule.upstream_category(&linter), rule))
            .into_group_map();

        if rules_by_upstream_category.len() > 1 {
            for (opt, rules) in &rules_by_upstream_category {
                if opt.is_some() {
                    let UpstreamCategoryAndPrefix { category, prefix } = opt.unwrap();
                    table_out.push_str(&format!("#### {category} ({prefix})"));
                }
                table_out.push('\n');
                table_out.push('\n');
                generate_table(&mut table_out, rules.clone(), &linter);
            }
        } else {
            generate_table(&mut table_out, linter.all_rules(), &linter);
        }
    }

    table_out
}
