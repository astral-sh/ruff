use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::scope::{FunctionDef, ScopeKind};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for usages of `await` outside of `async` functions.
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
/// - [Python documentation](https://docs.python.org/3/reference/expressions.html#await)
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
    if !checker
        .semantic_model()
        .scopes()
        .find_map(|scope| {
            if let ScopeKind::Function(FunctionDef { async_, .. }) = &scope.kind {
                Some(*async_)
            } else {
                None
            }
        })
        .unwrap_or(true)
    {
        checker
            .diagnostics
            .push(Diagnostic::new(AwaitOutsideAsync, expr.range()));
    }
}
