use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::TextRange;

use crate::AlwaysFixableViolation;
use crate::checkers::ast::LintContext;
use crate::codes::Category;

/// ## What it does
///
/// Checks Ruff configuration files for redirected rule names, codes, and prefixes.
///
/// ## Why is this bad?
///
/// A redirected selector still selects the same rules as its replacement. Using the current
/// selector makes it easier to find those rules in Ruff's documentation.
///
/// ## Example
///
/// ```toml
/// [tool.ruff.lint]
/// select = ["TCH"]
/// ignore = ["TCH002"]
/// ```
///
/// Use instead:
/// ```toml
/// [tool.ruff.lint]
/// select = ["TC"]
/// ignore = ["TC002"]
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct RenamedRule {
    original: &'static str,
    target: &'static str,
    setting: &'static str,
    in_lint_table: bool,
}

impl AlwaysFixableViolation for RenamedRule {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            original,
            target,
            setting,
            in_lint_table,
        } = self;
        if *in_lint_table {
            format!("Rule selector `{original}` redirects to `{target}` in `lint.{setting}`")
        } else {
            format!("Rule selector `{original}` redirects to `{target}` in `{setting}`")
        }
    }

    fn fix_title(&self) -> String {
        format!("Replace with `{}`", self.target)
    }
}

/// `RUF202`
pub(crate) fn renamed_rule(
    context: &LintContext,
    original: &'static str,
    target: &'static str,
    range: TextRange,
    setting: &'static str,
    in_lint_table: bool,
) {
    context
        .report_diagnostic(
            RenamedRule {
                original,
                target,
                setting,
                in_lint_table,
            },
            range,
        )
        .set_fix(Fix::safe_edit(Edit::range_replacement(
            target.to_string(),
            range,
        )));
}
