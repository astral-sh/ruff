use ruff_db::diagnostic::Diagnostic;
use toml::Spanned;
use toml::de::{DeArray, DeTable, DeValue};

use ruff_python_ast::TomlSourceType;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::checkers::ast::LintContext;
use crate::codes::Rule;
use crate::preview::is_human_readable_names_enabled;
use crate::rule_redirects::get_redirect;
use crate::rules::ruff::rules::{renamed_rule, renamed_rule_on_cli, rule_codes_in_selectors};

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

    visit_selector_arrays(ruff, |values, setting, in_lint_table| {
        check_selector_array(context, values, setting, in_lint_table);
    });
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

fn visit_selector_arrays(
    ruff: &DeTable<'_>,
    mut visit: impl FnMut(&DeArray<'_>, &'static str, bool),
) {
    let lint = ruff
        .get("lint")
        .and_then(|value| value.get_ref().as_table());
    for (table, in_lint_table) in [(Some(ruff), false), (lint, true)] {
        let Some(table) = table else { continue };
        for &setting in ARRAY_SELECTORS {
            if let Some(values) = table
                .get(setting)
                .and_then(|value| value.get_ref().as_array())
            {
                visit(values, setting, in_lint_table);
            }
        }
        for &setting in TABLE_SELECTORS {
            let Some(patterns) = table
                .get(setting)
                .and_then(|value| value.get_ref().as_table())
            else {
                continue;
            };
            for value in patterns.values() {
                if let Some(values) = value.get_ref().as_array() {
                    visit(values, setting, in_lint_table);
                }
            }
        }
    }
}

/// Check selectors in an inline TOML override supplied with `--config`.
pub fn renamed_rules_in_cli_config(table: &DeTable<'_>) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    visit_selector_arrays(table, |values, _, _| {
        for value in values {
            if let Some(selector) = value.get_ref().as_str()
                && let Some(diagnostic) = renamed_rule_on_cli(selector, "config")
            {
                diagnostics.push(diagnostic);
            }
        }
    });
    diagnostics
}

fn check_selector_array(
    context: &LintContext,
    values: &DeArray<'_>,
    setting: &'static str,
    in_lint_table: bool,
) {
    let source = context.source_file().source_text();

    for value in values {
        let Some(selector) = value.get_ref().as_str() else {
            continue;
        };
        let redirect = get_redirect(selector);
        if context.is_rule_enabled(Rule::RenamedRule)
            && let Some((original, target)) = redirect
        {
            let range = selector_range(value, source);
            renamed_rule(context, original, target, range, setting, in_lint_table);
        }

        let code = redirect.map_or(selector, |(_, target)| target);
        if context.is_rule_enabled(Rule::RuleCodesInSelectors)
            && is_human_readable_names_enabled(context.settings().preview)
            && let Ok(rule) = Rule::from_code(code)
        {
            let range = selector_range(value, source);
            rule_codes_in_selectors(context, rule.name(), range, setting, in_lint_table);
        }
    }
}

/// Return the source range of the selector inside its TOML quotes.
///
/// Callers must first validate the decoded string as a rule code or redirect.
/// This ensures that all leading quote characters belong to the TOML delimiter.
///
/// ```toml
/// [lint]
/// select = ["F401"]
///            ^^^^
/// ```
fn selector_range(spanned: &Spanned<DeValue<'_>>, source: &str) -> TextRange {
    let span = spanned.span();
    let range = TextRange::new(
        TextSize::try_from(span.start).unwrap(),
        TextSize::try_from(span.end).unwrap(),
    );

    let string = &source[range];
    let content = string.trim_start_matches(['"', '\'']);
    let quote_len = string.text_len() - content.text_len();
    let start = range.start() + quote_len;
    let end = range.end() - quote_len;
    TextRange::new(start, end)
}
