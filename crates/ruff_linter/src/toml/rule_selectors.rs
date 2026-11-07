use toml::Spanned;
use toml::de::{DeArray, DeTable, DeValue};

use ruff_db::diagnostic::LintName;
use ruff_python_ast::TomlSourceType;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::checkers::ast::LintContext;
use crate::codes::Rule;
use crate::preview::is_human_readable_names_enabled;
use crate::rule_redirects::get_redirect;
use crate::rules::ruff::rules::{redirected_rule_selector, rule_codes_in_selectors};

pub(super) fn check_rule_selectors(
    context: &LintContext,
    document: &DeTable<'_>,
    source_type: TomlSourceType,
) {
    let ruff = match source_type {
        TomlSourceType::Pyproject => document
            .get("tool")
            .and_then(|tool| tool.get_ref().get("ruff"))
            .and_then(|ruff| ruff.get_ref().as_table()),
        TomlSourceType::Ruff => Some(document),
        _ => None,
    };

    let Some(ruff) = ruff else {
        return;
    };

    check_selectors(context, ruff, false);

    if let Some(lint) = ruff.get("lint").and_then(|lint| lint.get_ref().as_table()) {
        check_selectors(context, lint, true);
    }
}

/// Selectors that are themselves arrays.
///
/// For example:
///
/// ```toml
/// select = ["F401"]
/// ```
const ARRAY_SELECTORS: &[&str] = &[
    "select",
    "extend-select",
    "fixable",
    "extend-fixable",
    "ignore",
    "extend-ignore",
    "unfixable",
    "extend-unfixable",
    "extend-safe-fixes",
    "extend-unsafe-fixes",
];

/// Selectors that are tables containing arrays.
///
/// For example:
///
/// ```toml
/// per-file-ignores = { "*.py" = ["F401"] }
/// ```
const TABLE_SELECTORS: &[&str] = &["per-file-ignores", "extend-per-file-ignores"];

fn check_selectors(context: &LintContext, table: &DeTable<'_>, in_lint_table: bool) {
    for &selector in ARRAY_SELECTORS {
        let Some(value) = table.get(selector) else {
            continue;
        };

        if let DeValue::Array(values) = value.get_ref() {
            check_selector_array(context, values, selector, in_lint_table);
        }
    }

    for &selector in TABLE_SELECTORS {
        let Some(value) = table.get(selector) else {
            continue;
        };

        if let DeValue::Table(per_file) = value.get_ref() {
            for value in per_file.values() {
                let Some(values) = value.get_ref().as_array() else {
                    continue;
                };
                check_selector_array(context, values, selector, in_lint_table);
            }
        }
    }
}

fn check_selector_array(
    context: &LintContext,
    values: &DeArray<'_>,
    setting: &'static str,
    in_lint_table: bool,
) {
    let source = context.source_file().source_text();

    for value in values {
        let Some(selector) = Selector::from_spanned(value, source) else {
            continue;
        };
        if context.is_rule_enabled(Rule::RedirectedRuleSelector)
            && let Some((original, target)) = selector.redirect
        {
            redirected_rule_selector(
                context,
                original,
                target,
                selector.range,
                setting,
                in_lint_table,
            );
        }
        if context.is_rule_enabled(Rule::RuleCodesInSelectors)
            && is_human_readable_names_enabled(context.settings().preview)
            && let Some(name) = selector.name
        {
            rule_codes_in_selectors(context, name, selector.range, setting, in_lint_table);
        }
    }
}

struct Selector {
    name: Option<LintName>,
    redirect: Option<(&'static str, &'static str)>,
    range: TextRange,
}

impl Selector {
    /// Extract a rule selector and its range from a spanned TOML string.
    ///
    /// The range corresponds to the selector itself rather than the surrounding string:
    ///
    /// ```toml
    /// [lint]
    /// select = ["F401"]
    ///            ^^^^
    /// ```
    fn from_spanned(spanned: &Spanned<DeValue<'_>>, source: &str) -> Option<Self> {
        let selector = spanned.get_ref().as_str()?;
        let redirect = get_redirect(selector);
        let code = redirect.map_or(selector, |(_, target)| target);
        let name = Rule::from_code(code).ok().map(|rule| rule.name());
        if name.is_none() && redirect.is_none() {
            return None;
        }

        let span = spanned.span();
        let range = TextRange::new(
            TextSize::try_from(span.start).unwrap(),
            TextSize::try_from(span.end).unwrap(),
        );

        // Note that this should be infallible because the parsed TOML string is surrounded by valid
        // quotes, and its content was recognized above as a rule code or a redirect. This
        // means that we don't have to worry about stripping nested quotes like `"'F401'"` or similar.
        let range = {
            let string = &source[range];
            let content = string.trim_start_matches(['"', '\'']);
            let quote_len = string.text_len() - content.text_len();
            let start = range.start() + quote_len;
            let end = range.end() - quote_len;
            TextRange::new(start, end)
        };

        Some(Self {
            name,
            redirect,
            range,
        })
    }
}
