use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of `trio.sleep` in a `while` loop.
///
/// ## Why is this bad?
/// Instead of sleeping in a `while` loop, and waiting for a condition
/// to become true, it's preferable to `wait()` on a `trio.Event`.
///
/// ## Example
/// ```python
/// DONE = False
///
///
/// async def func():
///     while not DONE:
///         await trio.sleep(1)
/// ```
///
/// Use instead:
/// ```python
/// DONE = trio.Event()
///
///
/// async def func():
///     await DONE.wait()
/// ```
#[violation]
pub struct TrioUnneededSleep;

impl Violation for TrioUnneededSleep {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `trio.Event` instead of awaiting `trio.sleep` in a `while` loop")
    }
}

/// ASYNC110
pub(crate) fn unneeded_sleep(checker: &mut Checker, while_stmt: &ast::StmtWhile) {
    if !checker.semantic().seen_module(Modules::TRIO) {
        return;
    }

    // The body should be a single `await` call.
    let [stmt] = while_stmt.body.as_slice() else {
        return;
    };
    let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
        return;
    };
    let Expr::Await(ast::ExprAwait { value, .. }) = value.as_ref() else {
        return;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return;
    };

    if checker
        .semantic()
        .resolve_qualified_name(func.as_ref())
        .is_some_and(|path| matches!(path.segments(), ["trio", "sleep" | "sleep_until"]))
    {
        checker
            .diagnostics
            .push(Diagnostic::new(TrioUnneededSleep, while_stmt.range()));
    }
}
