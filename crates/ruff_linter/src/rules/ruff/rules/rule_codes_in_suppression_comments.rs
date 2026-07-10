use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::AlwaysFixableViolation;

/// ## What it does
///
/// Checks for rule codes in Ruff-specific suppression comments.
///
/// ## Why is this bad?
///
/// Human-readable rule names are easier to understand than rule codes. Using names also avoids
/// requiring readers to look up the meaning of each code.
///
/// This rule applies to `ruff:ignore`, `ruff:file-ignore`, `ruff:disable`, and `ruff:enable`
/// comments.
///
/// ## Example
///
/// ```python
/// import os  # ruff:ignore[F401]
/// ```
///
/// Use instead:
/// ```python
/// import os  # ruff:ignore[unused-import]
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct RuleCodesInSuppressionComments;

impl AlwaysFixableViolation for RuleCodesInSuppressionComments {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Rule code used instead of name in suppression comment".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace rule code with name".to_string()
    }
}
