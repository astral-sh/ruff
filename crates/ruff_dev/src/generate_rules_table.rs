//! Generate a Markdown-compatible table of supported lint rules.
//!
//! Used for <https://docs.astral.sh/ruff/rules/>.

use itertools::Itertools;
use ruff_linter::codes::RuleGroup;
use std::borrow::Cow;
use std::fmt::Write;
use strum::IntoEnumIterator;

use ruff_linter::FixAvailability;
use ruff_linter::registry::{Linter, Rule, RuleNamespace};
use ruff_linter::upstream_categories::UpstreamCategoryAndPrefix;
use ruff_options_metadata::OptionsMetadata;
use ruff_workspace::options::Options;

const FIX_SYMBOL: &str = "🛠️";
const PREVIEW_SYMBOL: &str = "🧪";
const REMOVED_SYMBOL: &str = "❌";
const WARNING_SYMBOL: &str = "⚠️";
const SPACER: &str = "&nbsp;&nbsp;&nbsp;&nbsp;";

/// Style for the rule's fixability and status icons.
const SYMBOL_STYLE: &str = "style='width: 1em; display: inline-block;'";
/// Style for the container wrapping the fixability and status icons.
const SYMBOLS_CONTAINER: &str = "style='display: flex; gap: 0.5rem; justify-content: end;'";

fn generate_table(table_out: &mut String, rules: impl IntoIterator<Item = Rule>, linter: &Linter) {
    table_out.push_str("| Code | Name | Message |    |");
    table_out.push('\n');
    table_out.push_str("| ---- | ---- | ------- | -: |");
    table_out.push('\n');
    for rule in rules {
        let status_token = match rule.group() {
            RuleGroup::Removed { since } => {
                format!(
                    "<span {SYMBOL_STYLE} title='Rule was removed in {since}'>{REMOVED_SYMBOL}</span>"
                )
            }
            RuleGroup::Deprecated { since } => {
                format!(
                    "<span {SYMBOL_STYLE} title='Rule has been deprecated since {since}'>{WARNING_SYMBOL}</span>"
                )
            }
            RuleGroup::Preview { since } => {
                format!(
                    "<span {SYMBOL_STYLE} title='Rule has been in preview since {since}'>{PREVIEW_SYMBOL}</span>"
                )
            }
            RuleGroup::Stable { since } => {
                format!("<span {SYMBOL_STYLE} title='Rule has been stable since {since}'></span>")
            }
        };

        let fix_token = match rule.fixable() {
            FixAvailability::Always | FixAvailability::Sometimes => {
                format!("<span {SYMBOL_STYLE} title='Automatic fix available'>{FIX_SYMBOL}</span>")
            }
            FixAvailability::None => format!("<span {SYMBOL_STYLE}></span>"),
        };

        let rule_name = rule.name();

        // If the message ends in a bracketed expression (like: "Use {replacement}"), escape the
        // brackets. Otherwise, it'll be interpreted as an HTML attribute via the `attr_list`
        // plugin. (Above, we'd convert to "Use {replacement\}".)
        let message = rule.message_formats()[0];
        let message = if let Some(prefix) = message.strip_suffix('}') {
            Cow::Owned(format!("{prefix}\\}}"))
        } else {
            Cow::Borrowed(message)
        };

        // Start and end of style spans
        let mut ss = "";
        let mut se = "";
        if rule.is_removed() {
            ss = "<span style='opacity: 0.5', title='This rule has been removed'>";
            se = "</span>";
        } else if rule.is_deprecated() {
            ss = "<span style='opacity: 0.8', title='This rule has been deprecated'>";
            se = "</span>";
        }

        #[expect(clippy::or_fun_call)]
        let _ = write!(
            table_out,
            "| {ss}{prefix}{code}{se} {{ #{prefix}{code} }} | {ss}{explanation}{se} | {ss}{message}{se} | <div {SYMBOLS_CONTAINER}>{status_token}{fix_token}</div>|",
            prefix = linter.common_prefix(),
            code = linter.code_for_rule(rule).unwrap(),
            explanation = rule
                .explanation()
                .is_some()
                .then_some(format_args!("[{rule_name}](rules/{rule_name}.md)"))
                .unwrap_or(format_args!("{rule_name}")),
        );
        table_out.push('\n');
    }
    table_out.push('\n');
}

pub(crate) fn generate() -> String {
    // Generate the table string.
    let mut table_out = String::new();

    table_out.push_str("### Legend");
    table_out.push('\n');

    let _ = write!(
        &mut table_out,
        "{SPACER}{PREVIEW_SYMBOL}{SPACER} The rule is unstable and is in [\"preview\"](faq.md#what-is-preview)."
    );
    table_out.push_str("<br />");

    let _ = write!(
        &mut table_out,
        "{SPACER}{WARNING_SYMBOL}{SPACER} The rule has been deprecated and will be removed in a future release."
    );
    table_out.push_str("<br />");

    let _ = write!(
        &mut table_out,
        "{SPACER}{REMOVED_SYMBOL}{SPACER} The rule has been removed only the documentation is available."
    );
    table_out.push_str("<br />");

    let _ = write!(
        &mut table_out,
        "{SPACER}{FIX_SYMBOL}{SPACER} The rule is automatically fixable by the `--fix` command-line option."
    );
    table_out.push_str("\n\n");
    table_out.push_str("All rules not marked as preview, deprecated or removed are stable.");
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
        let _ = write!(&mut table_out, "### {} ({codes_csv})", linter.name());
        table_out.push('\n');
        table_out.push('\n');

        if let Some(url) = linter.url() {
            let host = url
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap();
            let _ = write!(
                table_out,
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
            );
            table_out.push('\n');
            table_out.push('\n');
        }

        if Options::metadata().has(&format!("lint.{}", linter.name())) {
            let _ = write!(
                table_out,
                "For related settings, see [{}](settings.md#lint{}).",
                linter.name(),
                linter.name(),
            );
            table_out.push('\n');
            table_out.push('\n');
        }

        let rules_by_upstream_category = linter
            .all_rules()
            .map(|rule| (rule.upstream_category(&linter), rule))
            .into_group_map();

        let mut rules_by_upstream_category: Vec<_> = rules_by_upstream_category.iter().collect();

        // Sort the upstream categories alphabetically by prefix.
        rules_by_upstream_category.sort_by(|(a, _), (b, _)| {
            a.as_ref()
                .map(|category| category.prefix)
                .unwrap_or_default()
                .cmp(
                    b.as_ref()
                        .map(|category| category.prefix)
                        .unwrap_or_default(),
                )
        });

        if rules_by_upstream_category.len() > 1 {
            for (opt, rules) in rules_by_upstream_category {
                if opt.is_some() {
                    let UpstreamCategoryAndPrefix { category, prefix } = opt.unwrap();
                    match codes_csv.as_str() {
                        "PL" => {
                            let _ = write!(table_out, "#### {category} ({codes_csv}{prefix})");
                        }
                        _ => {
                            let _ = write!(table_out, "#### {category} ({prefix})");
                        }
                    }
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
