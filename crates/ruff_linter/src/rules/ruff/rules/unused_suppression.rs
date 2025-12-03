use crate::AlwaysFixableViolation;
use ruff_macros::{ViolationMetadata, derive_message_formats};

/// ## What it does
/// Checks for range suppressions that are no longer used.
///
/// ## Why is this bad?
/// A range suppression that no longer matches any diagnostic violations is
/// likely included by mistake, and should be removed to avoid confusion.
///
/// ## Example
/// ```python
/// # ruff: disable[F401]
/// import foo
///
///
/// def bar():
///     foo.bar()
/// ```
///
/// Use instead:
/// ```python
/// import foo
///
///
/// def bar():
///     foo.bar()
/// ```
///
/// ## Options
/// - `lint.external`
///
/// ## References
/// - [Ruff error suppression](https://docs.astral.sh/ruff/linter/#error-suppression)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.9")]
pub(crate) struct UnusedSuppression;

impl AlwaysFixableViolation for UnusedSuppression {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unused suppression".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove unused suppression".to_string()
    }
}
