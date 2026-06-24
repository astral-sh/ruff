use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

/// ## What it does
/// Checks for `break` statements outside of loops.
///
/// ## Why is this bad?
/// The use of a `break` statement outside of a `for` or `while` loop will
/// raise a `SyntaxError`.
///
/// ## Example
/// ```python
/// def foo():
///     break
/// ```
///
/// ## References
/// - [Python documentation: `break`](https://docs.python.org/3/reference/simple_stmts.html#the-break-statement)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.36")]
pub(crate) struct BreakOutsideLoop;

impl Violation for BreakOutsideLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`break` outside loop".to_string()
    }
}
