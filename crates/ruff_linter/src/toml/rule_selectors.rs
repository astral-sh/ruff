use toml::Spanned;
use toml::de::{DeArray, DeTable, DeValue};

use ruff_python_ast::TomlSourceType;
use ruff_python_ast::script::ScriptTag;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::checkers::ast::LintContext;
use crate::codes::Rule;
use crate::preview::is_human_readable_names_enabled;
use crate::rule_redirects::get_redirect;
use crate::rules::ruff::rules::{renamed_rule, rule_codes_in_selectors};

pub(super) fn check_rule_selectors(
    context: &LintContext,
    document: &DeTable<'_>,
    source_type: TomlSourceType,
    source: SelectorSource,
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

    check_selectors(context, ruff, false, source);

    if let Some(lint) = ruff.get("lint").and_then(|lint| lint.get_ref().as_table()) {
        check_selectors(context, lint, true, source);
    }
}

#[derive(Clone, Copy)]
pub(super) enum SelectorSource<'a> {
    Toml(&'a str),
    Script(&'a ScriptTag),
}

impl SelectorSource<'_> {
    fn selector_range(self, value: &Spanned<DeValue<'_>>) -> TextRange {
        match self {
            Self::Toml(source) => selector_range(value, source),
            Self::Script(tag) => tag
                .source_map()
                .map_range(selector_range(value, tag.metadata())),
        }
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

fn check_selectors(
    context: &LintContext,
    table: &DeTable<'_>,
    in_lint_table: bool,
    source: SelectorSource,
) {
    for &selector in ARRAY_SELECTORS {
        let Some(value) = table.get(selector) else {
            continue;
        };

        if let DeValue::Array(values) = value.get_ref() {
            check_selector_array(context, values, selector, in_lint_table, source);
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
                check_selector_array(context, values, selector, in_lint_table, source);
            }
        }
    }
}

fn check_selector_array(
    context: &LintContext,
    values: &DeArray<'_>,
    setting: &'static str,
    in_lint_table: bool,
    source: SelectorSource,
) {
    for value in values {
        let Some(selector) = value.get_ref().as_str() else {
            continue;
        };
        let redirect = get_redirect(selector);
        if context.is_rule_enabled(Rule::RenamedRule)
            && let Some((original, target)) = redirect
        {
            let range = source.selector_range(value);
            renamed_rule(context, original, target, range, setting, in_lint_table);
        }

        let code = redirect.map_or(selector, |(_, target)| target);
        if context.is_rule_enabled(Rule::RuleCodesInSelectors)
            && is_human_readable_names_enabled(context.settings().preview)
            && let Ok(rule) = Rule::from_code(code)
        {
            let range = source.selector_range(value);
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
