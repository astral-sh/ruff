use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

/// ## What it does
/// Checks for unmatched range suppression comments
///
/// ## Why is this bad?
/// Unmatched range suppression comments can inadvertently suppress violations
/// over larger sections of code than intended, particularly at module scope.
///
/// ## Example
/// ```python
/// def foo():
///     ruff: disable[E501]  # unmatched
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     # ruff: disable[E501]
///     ...
///     # ruff: enable[E501]
/// ```
///
/// ## References
/// - [Ruff error suppression](https://docs.astral.sh/ruff/linter/#error-suppression)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.9")]
pub(crate) struct UnmatchedSuppressionComment;

impl Violation for UnmatchedSuppressionComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unmatched suppression comment".to_string()
    }
}
