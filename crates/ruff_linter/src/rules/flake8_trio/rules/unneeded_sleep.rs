use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprAwait, ExprCall, Stmt, StmtExpr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for trio functions that should contain await but don't.
///
/// ## Why is this bad?
/// Some trio context managers, such as `trio.fail_after` and
/// `trio.move_on_after`, have no effect unless they contain an `await`
/// statement. The use of such functions without an `await` statement is
/// likely a mistake.
///
/// ## Example
/// ```python
/// async def func():
///     with trio.move_on_after(2):
///         do_something()
/// ```
///
/// Use instead:
/// ```python
/// async def func():
///     with trio.move_on_after(2):
///         do_something()
///         await awaitable()
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
