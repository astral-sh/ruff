use std::fmt;

use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `asyncio.create_task` and `asyncio.ensure_future` calls
/// that do not store a reference to the returned result.
///
/// ## Why is this bad?
/// Per the `asyncio` documentation, the event loop only retains a weak
/// reference to tasks. If the task returned by `asyncio.create_task` and
/// `asyncio.ensure_future` is not stored in a variable, or a collection,
/// or otherwise referenced, it may be garbage collected at any time. This
/// can lead to unexpected and inconsistent behavior, as your tasks may or
/// may not run to completion.
///
/// ## Example
/// ```python
/// import asyncio
///
/// for i in range(10):
///     # This creates a weak reference to the task, which may be garbage
///     # collected at any time.
///     asyncio.create_task(some_coro(param=i))
/// ```
///
/// Use instead:
/// ```python
/// import asyncio
///
/// background_tasks = set()
///
/// for i in range(10):
///     task = asyncio.create_task(some_coro(param=i))
///
///     # Add task to the set. This creates a strong reference.
///     background_tasks.add(task)
///
///     # To prevent keeping references to finished tasks forever,
///     # make each task remove its own reference from the set after
///     # completion:
///     task.add_done_callback(background_tasks.discard)
/// ```
///
/// ## References
/// - [_The Heisenbug lurking in your async code_](https://textual.textualize.io/blog/2023/02/11/the-heisenbug-lurking-in-your-async-code/)
/// - [The Python Standard Library](https://docs.python.org/3/library/asyncio-task.html#asyncio.create_task)
#[violation]
pub struct AsyncioDanglingTask {
    method: Method,
}

impl Violation for AsyncioDanglingTask {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AsyncioDanglingTask { method } = self;
        format!("Store a reference to the return value of `asyncio.{method}`")
    }
}

/// RUF006
pub(crate) fn asyncio_dangling_task(checker: &mut Checker, expr: &Expr) {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return;
    };

    let Some(method) = checker
        .semantic()
        .resolve_call_path(func)
        .and_then(|call_path| match call_path.as_slice() {
            ["asyncio", "create_task"] => Some(Method::CreateTask),
            ["asyncio", "ensure_future"] => Some(Method::EnsureFuture),
            _ => None,
        })
    else {
        return;
    };

    checker.diagnostics.push(Diagnostic::new(
        AsyncioDanglingTask { method },
        expr.range(),
    ));
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Method {
    CreateTask,
    EnsureFuture,
}

impl fmt::Display for Method {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Method::CreateTask => fmt.write_str("create_task"),
            Method::EnsureFuture => fmt.write_str("ensure_future"),
        }
    }
}
