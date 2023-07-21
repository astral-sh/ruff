use rustpython_parser::ast::{ExprYieldFrom, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

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
#[violation]
pub struct YieldFromInAsyncFunction;

impl Violation for YieldFromInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`yield from` statement in async function; use `async for` instead")
    }
}

/// PLE1700
pub(crate) fn yield_from_in_async_function(checker: &mut Checker, expr: &ExprYieldFrom) {
    let scope = checker.semantic().scope();
    if scope.kind.is_async_function() {
        checker
            .diagnostics
            .push(Diagnostic::new(YieldFromInAsyncFunction, expr.range()));
    }
}
