use itertools::Itertools;

use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::{
    FixAvailability, Locator, Violation, checkers::ast::LintContext, codes::Rule, noqa::Directive,
    suppression::Suppressions,
};

/// ## What it does
///
/// Checks for the use of `noqa` comments instead of Ruff-specific `ruff:ignore` comments.
///
/// ## Why is this bad?
///
/// `ruff:ignore` comments allow the use of rule names instead of codes and can be used in more
/// places than `noqa` comments.
///
/// Note that this is an opinionated, stylistic rule. `noqa` comments may be needed for backwards
/// compatibility with other tools. You should also feel free to disable this rule if you simply
/// prefer `noqa` comments.
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
///
/// ## Options
///
/// This rule will flag `noqa` comments containing rule codes that are unknown to Ruff, even if they
/// are valid for other tools. You can tell Ruff to ignore such codes by configuring the list of
/// known "external" rule codes with the following option:
///
/// - `lint.external`
///
/// Ruff will still emit a diagnostic without a fix if `external` and known codes are present in the
/// same `noqa` comment, assuming that only the `external` codes need to remain in the `noqa`
/// comment.
///
/// ## See also
///
/// This rule avoids offering a fix if any of the rule codes in a `noqa` comment are unused. See
/// `unused-noqa` for a rule that will remove these and allow the remaining codes to be moved into a
/// `ruff:ignore` comment.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct NoqaComment {
    file_level: bool,
    has_codes: bool,
}

impl Violation for NoqaComment {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if !self.file_level {
            "`noqa` comment used instead of `ruff:ignore`".to_string()
        } else {
            "`ruff: noqa` comment used instead of `ruff:file-ignore`".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        if !self.has_codes {
            return None;
        }

        Some(if self.file_level {
            "Use `ruff:file-ignore` instead".to_string()
        } else {
            "Use `ruff:ignore` instead".to_string()
        })
    }
}

/// RUF105
pub(crate) fn noqa_comment(
    context: &LintContext,
    locator: &Locator,
    file_level: bool,
    has_unused_codes: bool,
    directive: &Directive,
    suppressions: &Suppressions,
) {
    let range = directive.range();

    if file_level && locator.slice(range).contains("flake8") {
        return;
    }

    let has_external_codes = if let Directive::Codes(codes) = directive {
        let external_codes = codes
            .iter()
            .filter(|code| {
                context
                    .settings()
                    .external
                    .iter()
                    .any(|prefix| code.as_str().starts_with(prefix))
            })
            .count();

        // Avoid a diagnostic if all of the codes are external.
        if external_codes == codes.len() {
            return;
        }

        external_codes > 0
    } else {
        false
    };

    if suppressions.check_rule(Rule::NoqaComment, range, None) {
        return;
    }

    let has_codes = matches!(directive, Directive::Codes(_));
    let mut diagnostic = context.report_diagnostic(
        NoqaComment {
            file_level,
            has_codes,
        },
        range,
    );

    // If some codes are external, return without a fix.
    if has_external_codes {
        return;
    }

    // Similarly, return without a fix if any unused codes are present. This avoids potentially
    // activating an unused `noqa` comment on its own line like:
    //
    // ```py
    // # noqa: F401
    // import math
    // ```
    //
    // by converting it to a valid `ruff:ignore` comment.
    if has_unused_codes {
        return;
    }

    let Directive::Codes(codes) = directive else {
        return;
    };

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
