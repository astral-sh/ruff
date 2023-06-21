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
/// def foo(bar=1):  # noqa: ARG001
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar=1):  # noqa: ARG001  # We'll need arg `bar` a future version.
///     ...
/// ```
///
/// ## References
/// - [Ruff documentation: Error suppression](https://beta.ruff.rs/docs/configuration/#error-suppression)
#[violation]
pub struct UnexplainedNOQA {
    pub directive: String,
}

impl Violation for UnexplainedNOQA {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnexplainedNOQA { directive } = self;
        format!(
            "Unexplained `noqa` directive, consider adding an explanation comment to `{directive}`"
        )
    }
}
