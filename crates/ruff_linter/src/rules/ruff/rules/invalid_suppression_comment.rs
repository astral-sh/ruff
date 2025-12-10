use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::AlwaysFixableViolation;

/// ## What it does
/// Checks for invalid suppression comments
///
/// ## Why is this bad?
/// Invalid suppression comments are ignored by Ruff, and should either
/// be fixed or removed to avoid confusion.
///
/// ## Example
/// ```python
/// ruff: disable  # missing codes
/// ```
///
/// Use instead:
/// ```python
/// # ruff: disable[E501]
/// ```
///
/// Or delete the invalid suppression comment.
///
/// ## References
/// - [Ruff error suppression](https://docs.astral.sh/ruff/linter/#error-suppression)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.9")]
pub(crate) struct InvalidSuppressionComment;

impl AlwaysFixableViolation for InvalidSuppressionComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Invalid suppression comment".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove invalid suppression comment".to_string()
    }
}
