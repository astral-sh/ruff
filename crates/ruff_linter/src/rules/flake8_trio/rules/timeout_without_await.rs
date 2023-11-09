use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::AwaitVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{StmtWith, WithItem};

use crate::checkers::ast::Checker;
use crate::rules::flake8_trio::method_name::MethodName;

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

    if !matches!(
        method_name,
        MethodName::MoveOnAfter
            | MethodName::MoveOnAt
            | MethodName::FailAfter
            | MethodName::FailAt
            | MethodName::CancelScope
    ) {
        return;
    }

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
