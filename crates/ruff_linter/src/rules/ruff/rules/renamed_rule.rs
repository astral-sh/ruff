use std::borrow::Cow;

use ruff_db::diagnostic::{Diagnostic, DiagnosticId, SecondaryCode, Severity};
use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::TextRange;

use crate::checkers::ast::LintContext;
use crate::codes::Category;
use crate::registry::Rule;
use crate::rule_redirects::get_redirect;
use crate::{FixAvailability, Violation};

/// ## What it does
///
/// Checks Ruff configuration files and command line arguments for redirected rule names, codes, and
/// prefixes.
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
#[violation_metadata(preview_since = "0.17.5", category = Category::Suspicious)]
pub(crate) struct RenamedRule {
    original: &'static str,
    target: &'static str,
    setting: Cow<'static, str>,
    in_lint_table: bool,
}

impl Violation for RenamedRule {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

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

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with `{}`", self.target))
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
                setting: Cow::Borrowed(setting),
                in_lint_table,
            },
            range,
        )
        .set_fix(Fix::safe_edit(Edit::range_replacement(
            target.to_string(),
            range,
        )));
}

/// Check a rule selector passed to a command-line option.
pub fn renamed_rule_on_cli(selector: &str, option: &str) -> Option<Diagnostic> {
    let (original, target) = get_redirect(selector)?;
    let violation = RenamedRule {
        original,
        target,
        setting: Cow::Owned(format!("--{option}")),
        in_lint_table: false,
    };
    let rule = Rule::RenamedRule;
    let mut diagnostic = Diagnostic::new(
        DiagnosticId::Lint(rule.name()),
        Severity::Error,
        violation.message(),
    );
    if let Some(code) = rule.noqa_code() {
        diagnostic.set_secondary_code(SecondaryCode::new(code.to_string()));
    }
    diagnostic.set_documentation_url(Some(rule.url()));
    Some(diagnostic)
}
