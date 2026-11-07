use ruff_db::diagnostic::LintName;
use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::TextRange;

use crate::AlwaysFixableViolation;
use crate::checkers::ast::LintContext;
use crate::codes::Category;

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
#[violation_metadata(preview_since = "0.15.22", category = Category::Pedantic)]
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
    name: LintName,
    range: TextRange,
    selector: &'static str,
    in_lint_table: bool,
) {
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
