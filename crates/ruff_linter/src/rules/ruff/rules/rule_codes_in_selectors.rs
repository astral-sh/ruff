use toml::Spanned;
use toml::de::{DeArray, DeTable, DeValue};

use ruff_db::diagnostic::LintName;
use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::TomlSourceType;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::{
    AlwaysFixableViolation, checkers::ast::LintContext, codes::Rule,
    preview::is_human_readable_names_enabled, rule_redirects::get_redirect_target,
};

/// ## What it does
///
/// Checks for any configuration files that use rule codes as selectors.
///
/// ## Why is this bad?
///
/// Human-readable rule names are easier to understand than rule codes. Using names also avoids
/// requiring readers to look up the meaning of each code.
///
/// ## Example
///
/// ```toml
/// [tool.ruff.lint]
/// select = ["F401"]
/// ```
///
/// Use instead:
///
/// ```toml
/// [tool.ruff.lint]
/// select = ["unused-import"]
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct RuleCodesInSelectors {
    selector: &'static str,
    name: &'static str,
    in_lint_table: bool,
}

impl AlwaysFixableViolation for RuleCodesInSelectors {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            selector,
            in_lint_table,
            name: _,
        } = self;
        if *in_lint_table {
            format!("Rule code used instead of name in `lint.{selector}`")
        } else {
            format!("Rule code used instead of name in `{selector}`")
        }
    }

    fn fix_title(&self) -> String {
        format!("Replace rule code with `{name}`", name = self.name)
    }
}

/// RUF201
pub(crate) fn rule_codes_in_selectors(
    context: &LintContext,
    document: &DeTable<'_>,
    source_type: TomlSourceType,
) {
    if !is_human_readable_names_enabled(context.settings().preview) {
        return;
    }

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
    selector: &'static str,
    in_lint_table: bool,
) {
    let source = context.source_file().source_text();

    for value in values {
        let Some(RuleCode { name, range }) = RuleCode::from_spanned(value, source) else {
            continue;
        };

        context
            .report_diagnostic(
                RuleCodesInSelectors {
                    selector,
                    in_lint_table,
                    name: name.as_str(),
                },
                range,
            )
            .set_fix(Fix::safe_edit(Edit::range_replacement(
                name.to_string(),
                range,
            )));
    }
}

struct RuleCode {
    name: LintName,
    range: TextRange,
}

impl RuleCode {
    /// Extract a rule code and its range from a spanned TOML string.
    ///
    /// The range corresponds to the code itself rather than the surrounding string:
    ///
    /// ```toml
    /// [lint]
    /// select = ["F401"]
    ///            ^^^^
    /// ```
    fn from_spanned(spanned: &Spanned<DeValue<'_>>, source: &str) -> Option<Self> {
        let code = spanned.get_ref().as_str()?;
        let code = get_redirect_target(code).unwrap_or(code);
        let rule = Rule::from_code(code).ok()?;

        let span = spanned.span();
        let range = TextRange::new(
            TextSize::try_from(span.start).unwrap(),
            TextSize::try_from(span.end).unwrap(),
        );

        // Note that this should be infallible because the parsed TOML string is surrounded by valid
        // quotes, and `Rule::from_code` above guarantees that its content is a valid rule code. This
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
            name: rule.name(),
            range,
        })
    }
}
