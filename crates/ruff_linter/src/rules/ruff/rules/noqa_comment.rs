use itertools::Itertools;

use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::{
    AlwaysFixableViolation, Locator,
    checkers::ast::LintContext,
    codes::Rule,
    noqa::{Code, Codes},
    rule_redirects::get_redirect_target,
};

/// ## What it does
///
/// Checks for the use of `noqa` comments instead of Ruff-specific `ruff:ignore` comments.
///
/// ## Why is this bad?
///
/// `ruff:ignore` comments allow the use of rule names instead of codes and can be used in more
/// places than `noqa` comments. `noqa` comments should be used only for backwards compatibility
/// with other tools.
///
/// ## Example
///
/// ```python
/// import os  # noqa: F401
/// ```
///
/// Use instead:
/// ```python
/// import os  # ruff:ignore[F401]
/// ```
///
/// Or if you prefer the own-line form:
///
/// ```python
/// # ruff:ignore[unused-import]
/// import os
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "NEXT_RUFF_VERSION")]
pub(crate) struct NoqaComment {
    file_level: bool,
}

impl AlwaysFixableViolation for NoqaComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        if !self.file_level {
            "`noqa` comment used instead of `ruff:ignore`".to_string()
        } else {
            "`ruff: noqa` comment used instead of `ruff:file-ignore`".to_string()
        }
    }

    fn fix_title(&self) -> String {
        if self.file_level {
            "Use `ruff:file-ignore` instead".to_string()
        } else {
            "Use `ruff:ignore` instead".to_string()
        }
    }
}

/// RUF105
pub(crate) fn noqa_comment(
    context: &LintContext,
    locator: &Locator,
    file_level: bool,
    codes: &Codes,
) {
    if file_level && locator.slice(codes.range()).contains("flake8") {
        return;
    }

    // Skip cases with unknown codes, external or otherwise.
    if !codes.iter().all(is_valid_code) {
        return;
    }

    let mut diagnostic = context.report_diagnostic(NoqaComment { file_level }, codes.range());

    let edit = Edit::range_replacement(
        format!(
            "# ruff:{action}[{codes}]",
            action = if file_level { "file-ignore" } else { "ignore" },
            codes = codes.iter().join(", ")
        ),
        codes.range(),
    );
    diagnostic.set_fix(Fix::safe_edit(edit));
}

fn is_valid_code(code: &Code) -> bool {
    let code = code.as_str();
    let redirect = get_redirect_target(code).unwrap_or(code);
    Rule::from_code(redirect).is_ok()
}
