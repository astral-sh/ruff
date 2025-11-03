use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

/// ## What it does
/// Checks for `continue` statements outside of loops.
///
/// ## Why is this bad?
/// The use of a `continue` statement outside of a `for` or `while` loop will
/// raise a `SyntaxError`.
///
/// ## Example
/// ```python
/// def foo():
///     continue  # SyntaxError
/// ```
///
/// ## References
/// - [Python documentation: `continue`](https://docs.python.org/3/reference/simple_stmts.html#the-continue-statement)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.36")]
pub(crate) struct ContinueOutsideLoop;

impl Violation for ContinueOutsideLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`continue` not properly in loop".to_string()
    }
}
