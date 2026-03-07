use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ExprAwait, ExprCall, Keyword, StmtExpr, helpers::is_const_true};
use ruff_text_size::Ranged;

use crate::{Violation, checkers::ast::Checker};
use ruff_python_ast::Expr;

/// ## What it does
/// Checks for the use of asyncio.gather to collect exceptions as values and
/// ignoring the result.
///
/// ## Why is this bad?
/// This is the same as ignoring all possible exceptions in all executed tasks.
/// Ignoring Exceptions should always be explicit if intended.
///
/// ## Example
///
/// ```python
/// import asyncio
///
///
/// async def func():
///     raise ValueError
///
///
/// async def main():
///     # implicitly ignores all exceptions
///     await asyncio.gather(func(), return_exceptions=True)
/// ```
///
/// Use instead:
///
/// ```python
/// import asyncio
///
///
/// async def func():
///     raise ValueError
///
///
/// async def main():
///     await asyncio.gather(func())
///     # OR
///     _ = await asyncio.gather(func(), return_exceptions=True)
/// ```
///
/// ## References
/// - [`asyncio.gather`](https://docs.python.org/3/library/asyncio-task.html#asyncio.gather)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct AsyncioGatherIgnoredExceptions;

impl Violation for AsyncioGatherIgnoredExceptions {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not ignore potential error results".to_string()
    }
}

// RUF071
pub(crate) fn check_for_ignored_asyncio_gather(checker: &Checker, expr: &StmtExpr) {
    let Expr::Await(ExprAwait { value, .. }) = expr.value.as_ref() else {
        return;
    };
    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = value.as_ref()
    else {
        return;
    };

    if !is_asyncio_gather(checker, func) {
        return;
    }

    if arguments.keywords.iter().any(|Keyword { value, arg, .. }| {
        arg.as_ref().is_some_and(|arg| arg == "return_exceptions") && is_const_true(value)
    }) {
        checker.report_diagnostic(AsyncioGatherIgnoredExceptions, expr.range());
    }
}

fn is_asyncio_gather(checker: &Checker, func: &Expr) -> bool {
    checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["asyncio", "gather"]))
}
