use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::CallPath;
use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};
use ruff_python_ast::{Expr, ExprAwait, Stmt, StmtWith, WithItem};

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
pub struct TrioTimeoutWithoutAwait {
    method_name: MethodName,
}

impl Violation for TrioTimeoutWithoutAwait {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { method_name } = self;
        format!("A `with {method_name}(...):` context does not contain any `await` statements. This makes it pointless, as the timeout can only be triggered by a checkpoint.")
    }
}

/// TRIO100
pub(crate) fn timeout_without_await(
    checker: &mut Checker,
    with_stmt: &StmtWith,
    with_items: &[WithItem],
) {
    let Some(method_name) = with_items.iter().find_map(|item| {
        let call = item.context_expr.as_call_expr()?;
        let call_path = checker.semantic().resolve_call_path(call.func.as_ref())?;
        MethodName::try_from(&call_path)
    }) else {
        return;
    };

    let mut visitor = AwaitVisitor::default();
    visitor.visit_body(&with_stmt.body);
    if visitor.seen_await {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        TrioTimeoutWithoutAwait { method_name },
        with_stmt.range,
    ));
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MethodName {
    MoveOnAfter,
    MoveOnAt,
    FailAfter,
    FailAt,
    CancelScope,
}

impl MethodName {
    fn try_from(call_path: &CallPath<'_>) -> Option<Self> {
        match call_path.as_slice() {
            ["trio", "move_on_after"] => Some(Self::MoveOnAfter),
            ["trio", "move_on_at"] => Some(Self::MoveOnAt),
            ["trio", "fail_after"] => Some(Self::FailAfter),
            ["trio", "fail_at"] => Some(Self::FailAt),
            ["trio", "CancelScope"] => Some(Self::CancelScope),
            _ => None,
        }
    }
}

impl std::fmt::Display for MethodName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MethodName::MoveOnAfter => write!(f, "trio.move_on_after"),
            MethodName::MoveOnAt => write!(f, "trio.move_on_at"),
            MethodName::FailAfter => write!(f, "trio.fail_after"),
            MethodName::FailAt => write!(f, "trio.fail_at"),
            MethodName::CancelScope => write!(f, "trio.CancelScope"),
        }
    }
}

#[derive(Debug, Default)]
struct AwaitVisitor {
    seen_await: bool,
}

impl Visitor<'_> for AwaitVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => (),
            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Await(ExprAwait { .. }) = expr {
            self.seen_await = true;
        } else {
            walk_expr(self, expr);
        }
    }
}
