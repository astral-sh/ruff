use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprAwait, ExprCall, Stmt, StmtExpr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the while loop, which waits for the event.
///
/// ## Why is this bad?
/// Instead of sleeping in a loop waiting for a condition to be true,
/// it's preferable to use a trio. Event.
///
/// ## Example
/// ```python
/// DONE = False
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
/// async def func():
///     await DONE.wait()
/// ```
#[violation]
pub struct TrioUnneededSleep;

impl Violation for TrioUnneededSleep {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use event instead of `while <condition>: await trio.sleep()`")
    }
}

pub(crate) fn unneeded_sleep(checker: &mut Checker, stmt: &Stmt, body: &[Stmt]) {
    if body.len() != 1 {
        return;
    }

    let awaitable = {
        if let Stmt::Expr(StmtExpr { range: _, value }) = &body[0] {
            if let Expr::Await(ExprAwait { range: _, value }) = value.as_ref() {
                Some(value.as_ref())
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some(Expr::Call(ExprCall {
        range: _,
        func,
        arguments: _,
    })) = awaitable
    {
        if checker
            .semantic()
            .resolve_call_path(func.as_ref())
            .is_some_and(|path| matches!(path.as_slice(), ["trio", "sleep" | "sleep_until"]))
        {
            checker
                .diagnostics
                .push(Diagnostic::new(TrioUnneededSleep, stmt.range()));
        }
    }
}
