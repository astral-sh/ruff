use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `await` outside of `async` functions.
///
/// ## Why is this bad?
/// Using `await` outside of an `async` function is a syntax error.
///
/// ## Example
/// ```python
/// import asyncio
///
///
/// def foo():
///     await asyncio.sleep(1)
/// ```
///
/// Use instead:
/// ```python
/// import asyncio
///
///
/// async def foo():
///     await asyncio.sleep(1)
/// ```
///
/// ## References
/// - [Python documentation: Await expression](https://docs.python.org/3/reference/expressions.html#await)
/// - [PEP 492](https://peps.python.org/pep-0492/#await-expression)
#[violation]
pub struct AwaitOutsideAsync;

impl Violation for AwaitOutsideAsync {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`await` should be used within an async function")
    }
}

/// PLE1142
pub(crate) fn await_outside_async(checker: &mut Checker, expr: &Expr) {
    if !checker.semantic().in_async_context() {
        checker
            .diagnostics
            .push(Diagnostic::new(AwaitOutsideAsync, expr.range()));
    }
}
