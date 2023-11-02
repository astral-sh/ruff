use ruff_python_ast::visitor::{walk_expr, Visitor};
use ruff_python_ast::{Expr, ExprAwait, ExprCall, StmtWith, WithItem};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for trio functions that should contain await but don't.
///
/// ## Why is this bad?
///
/// Some trio context managers, such as 'trio.fail_after' and
/// 'trio.move_on_after', have no impact when there is no await statement in them.
///
/// ## Example
/// ```python
/// async def f():
///     with trio.move_on_after(2):
///         do_something()
/// ```
///
/// Use instead:
/// ```python
/// async def f():
///     with trio.move_on_after(2):
///         do_something()
///         await awaitable()
/// ```
#[violation]
pub struct TimeoutWithoutAwait;

impl Violation for TimeoutWithoutAwait {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The `await` statement should be included in the timeout context manager")
    }
}

struct AwaitVisitor {
    await_visited: bool,
}

impl Visitor<'_> for AwaitVisitor {
    fn visit_expr(&mut self, expr: &'_ ruff_python_ast::Expr) {
        if let Expr::Await(ExprAwait { .. }) = expr {
            self.await_visited = true;
        } else {
            walk_expr(self, expr);
        }
    }
}

/// TRIO100
pub(crate) fn timeout_without_await(
    checker: &mut Checker,
    with_stmt: &StmtWith,
    with_items: &[WithItem],
) {
    let mut visitor = AwaitVisitor {
        await_visited: false,
    };

    for item in with_items {
        if let Expr::Call(ExprCall {
            func,
            range: _,
            arguments: _,
        }) = &item.context_expr
        {
            if checker
                .semantic()
                .resolve_call_path(func.as_ref())
                .is_some_and(|mut path| {
                    matches!(
                        path.as_mut_slice(),
                        [
                            "trio",
                            "move_on_after"
                                | "move_on_at"
                                | "fail_after"
                                | "fail_at"
                                | "CancelScope"
                        ]
                    )
                })
            {
                for stmt in &with_stmt.body {
                    visitor.visit_stmt(stmt);
                }

                if !visitor.await_visited {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(TimeoutWithoutAwait, with_stmt.range));
                }
            }
        }
    }
}
