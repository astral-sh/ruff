use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

/// ## What it does
/// Checks for `return` statements outside of functions.
///
/// ## Why is this bad?
/// The use of a `return` statement outside of a function will raise a
/// `SyntaxError`.
///
/// ## Example
/// ```python
/// class Foo:
///     return 1
/// ```
///
/// ## References
/// - [Python documentation: `return`](https://docs.python.org/3/reference/simple_stmts.html#the-return-statement)
#[derive(ViolationMetadata)]
pub(crate) struct ReturnOutsideFunction;

impl Violation for ReturnOutsideFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`return` statement outside of a function/method".to_string()
    }
}
