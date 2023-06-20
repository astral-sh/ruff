use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for `noqa` directives that are not followed by an explanation.
///
/// ## Why is this bad?
/// A `noqa` directive without an explanation makes it difficult for the reader
/// to understand why the directive is necessary. Consider adding a comment
/// explaining why the directive is necessary.
///
/// ## Example
/// ```python
/// foo == ""  # noqa: PLC1901
/// ```
///
/// Use instead:
/// ```python
/// foo == ""  # noqa: PLC1901  # Check for empty string, not just falsiness.
/// ```
///
/// ## References
/// - [Ruff documentation: Error suppression](https://beta.ruff.rs/docs/configuration/#error-suppression)
#[violation]
pub struct UnexplainedNOQA;

impl Violation for UnexplainedNOQA {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unexplained `noqa` directive")
    }
}
