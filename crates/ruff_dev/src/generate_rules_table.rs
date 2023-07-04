//! Generate a Markdown-compatible table of supported lint rules.

use itertools::Itertools;
use strum::IntoEnumIterator;

use ruff::registry::{Linter, Rule, RuleNamespace, UpstreamCategory};
use ruff::settings::options::Options;
use ruff_diagnostics::AutofixKind;

const FIX_SYMBOL: &str = "🛠";

fn generate_table(table_out: &mut String, rules: impl IntoIterator<Item = Rule>, linter: &Linter) {
    table_out.push_str("| Code | Name | Message | Fix |");
    table_out.push('\n');
    table_out.push_str("| ---- | ---- | ------- | --- |");
    table_out.push('\n');
    for rule in rules {
        let fix_token = match rule.autofixable() {
            AutofixKind::None => "",
            AutofixKind::Always | AutofixKind::Sometimes => FIX_SYMBOL,
        };

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
            fix_token
        ));
        table_out.push('\n');
    }
    table_out.push('\n');
}

pub(crate) fn generate() -> String {
    // Generate the table string.
    let mut table_out = format!("The {FIX_SYMBOL} emoji indicates that a rule is automatically fixable by the `--fix` command-line option.\n\n");
    for linter in Linter::iter() {
        let codes_csv: String = match linter.common_prefix() {
            "" => linter
                .upstream_categories()
                .unwrap()
                .iter()
                .map(|UpstreamCategory(prefix, ..)| prefix.short_code())
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

        if let Some(categories) = linter.upstream_categories() {
            for UpstreamCategory(prefix, name) in categories {
                table_out.push_str(&format!(
                    "#### {name} ({}{})",
                    linter.common_prefix(),
                    prefix.short_code()
                ));
                table_out.push('\n');
                table_out.push('\n');
                generate_table(&mut table_out, prefix.clone().rules(), &linter);
            }
        } else {
            generate_table(&mut table_out, linter.rules(), &linter);
        }
    }

    table_out
}
