use std::fmt;

use ast::Stmt;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{analyze::typing, Scope, SemanticModel};
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
#[derive(ViolationMetadata)]
pub(crate) struct AsyncioDanglingTask {
    expr: String,
    method: Method,
}

impl Violation for AsyncioDanglingTask {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AsyncioDanglingTask { expr, method } = self;
        format!("Store a reference to the return value of `{expr}.{method}`")
    }
}

/// RUF006
pub(crate) fn asyncio_dangling_task(expr: &Expr, semantic: &SemanticModel) -> Option<Diagnostic> {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return None;
    };

    // Ex) `asyncio.create_task(...)`
    if let Some(method) = semantic
        .resolve_qualified_name(func)
        .and_then(|qualified_name| match qualified_name.segments() {
            ["asyncio", "create_task"] => Some(Method::CreateTask),
            ["asyncio", "ensure_future"] => Some(Method::EnsureFuture),
            _ => None,
        })
    {
        return Some(Diagnostic::new(
            AsyncioDanglingTask {
                expr: "asyncio".to_string(),
                method,
            },
            expr.range(),
        ));
    }

    // Ex) `loop = ...; loop.create_task(...)`
    if let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() {
        if attr == "create_task" {
            if let Expr::Name(name) = value.as_ref() {
                if typing::resolve_assignment(value, semantic).is_some_and(|qualified_name| {
                    matches!(
                        qualified_name.segments(),
                        [
                            "asyncio",
                            "get_event_loop" | "get_running_loop" | "new_event_loop"
                        ]
                    )
                }) {
                    return Some(Diagnostic::new(
                        AsyncioDanglingTask {
                            expr: name.id.to_string(),
                            method: Method::CreateTask,
                        },
                        expr.range(),
                    ));
                }
            }
        }
    }
    None
}

/// RUF006
pub(crate) fn asyncio_dangling_binding(scope: &Scope, checker: &Checker) {
    let semantic = checker.semantic();
    for binding_id in scope.binding_ids() {
        // If the binding itself is used, or it's not an assignment, skip it.
        let binding = semantic.binding(binding_id);
        if binding.is_used()
            || binding.is_global()
            || binding.is_nonlocal()
            || !binding.kind.is_assignment()
        {
            continue;
        }

        // Otherwise, flag any dangling tasks, including those that are shadowed, as in:
        // ```python
        // if x > 0:
        //     task = asyncio.create_task(make_request())
        // else:
        //     task = asyncio.create_task(make_request())
        // ```
        for binding_id in std::iter::successors(Some(binding_id), |id| scope.shadowed_binding(*id))
        {
            let binding = semantic.binding(binding_id);
            if binding.is_used()
                || binding.is_global()
                || binding.is_nonlocal()
                || !binding.kind.is_assignment()
            {
                continue;
            }

            let Some(source) = binding.source else {
                continue;
            };

            let diagnostic = match semantic.statement(source) {
                Stmt::Assign(ast::StmtAssign { value, targets, .. }) if targets.len() == 1 => {
                    asyncio_dangling_task(value, semantic)
                }
                Stmt::AnnAssign(ast::StmtAnnAssign {
                    value: Some(value), ..
                }) => asyncio_dangling_task(value, semantic),
                _ => None,
            };

            if let Some(diagnostic) = diagnostic {
                checker.report_diagnostic(diagnostic);
            }
        }
    }
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
