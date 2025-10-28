use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

/// ## What it does
/// Checks for uses of `yield from` in async functions.
///
/// ## Why is this bad?
/// Python doesn't support the use of `yield from` in async functions, and will
/// raise a `SyntaxError` in such cases.
///
/// Instead, considering refactoring the code to use an `async for` loop instead.
///
/// ## Example
/// ```python
/// async def numbers():
///     yield from [1, 2, 3, 4, 5]
/// ```
///
/// Use instead:
/// ```python
/// async def numbers():
///     async for number in [1, 2, 3, 4, 5]:
///         yield number
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.271")]
pub(crate) struct YieldFromInAsyncFunction;

impl Violation for YieldFromInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`yield from` statement in async function; use `async for` instead".to_string()
    }
}
