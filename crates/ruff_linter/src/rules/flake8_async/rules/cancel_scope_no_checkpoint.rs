use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::AwaitVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{StmtWith, WithItem};

use crate::checkers::ast::Checker;
use crate::rules::flake8_async::helpers::{AsyncModule, MethodName};
use crate::settings::types::PreviewMode;

/// ## What it does
/// Checks for timeout context managers which do not contain a checkpoint.
///
/// ## Why is this bad?
/// Some asynchronous context managers, such as `asyncio.timeout` and
/// `trio.move_on_after`, have no effect unless they contain a checkpoint.
/// The use of such context managers without an `await`, `async with` or
/// `async for` statement is likely a mistake.
///
/// ## Example
/// ```python
/// async def func():
///     with asyncio.timeout(2):
///         do_something()
/// ```
///
/// Use instead:
/// ```python
/// async def func():
///     with asyncio.timeout(2):
///         do_something()
///         await awaitable()
/// ```
///
/// ## References
/// - [`asyncio` timeouts](https://docs.python.org/3/library/asyncio-task.html#timeouts)
/// - [`anyio` timeouts](https://anyio.readthedocs.io/en/stable/cancellation.html)
/// - [`trio` timeouts](https://trio.readthedocs.io/en/stable/reference-core.html#cancellation-and-timeouts)
#[violation]
pub struct CancelScopeNoCheckpoint {
    method_name: MethodName,
}

impl Violation for CancelScopeNoCheckpoint {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { method_name } = self;
        format!("A `with {method_name}(...):` context does not contain any `await` statements. This makes it pointless, as the timeout can only be triggered by a checkpoint.")
    }
}

/// ASYNC100
pub(crate) fn cancel_scope_no_checkpoint(
    checker: &mut Checker,
    with_stmt: &StmtWith,
    with_items: &[WithItem],
) {
    let Some((with_item_pos, method_name)) = with_items
        .iter()
        .enumerate()
        .filter_map(|(pos, item)| {
            let call = item.context_expr.as_call_expr()?;
            let qualified_name = checker
                .semantic()
                .resolve_qualified_name(call.func.as_ref())?;
            let method_name = MethodName::try_from(&qualified_name)?;
            if method_name.is_timeout_context() {
                Some((pos, method_name))
            } else {
                None
            }
        })
        .last()
    else {
        return;
    };

    // If this is an `async with` and the timeout has items after it, then the
    // further items are checkpoints.
    if with_stmt.is_async && with_item_pos < with_items.len() - 1 {
        return;
    }

    // If the body contains an `await` statement, the context manager is used correctly.
    let mut visitor = AwaitVisitor::default();
    visitor.visit_body(&with_stmt.body);
    if visitor.seen_await {
        return;
    }

    if matches!(checker.settings.preview, PreviewMode::Disabled) {
        if matches!(method_name.module(), AsyncModule::Trio) {
            checker.diagnostics.push(Diagnostic::new(
                CancelScopeNoCheckpoint { method_name },
                with_stmt.range,
            ));
        }
    } else {
        checker.diagnostics.push(Diagnostic::new(
            CancelScopeNoCheckpoint { method_name },
            with_stmt.range,
        ));
    }
}
