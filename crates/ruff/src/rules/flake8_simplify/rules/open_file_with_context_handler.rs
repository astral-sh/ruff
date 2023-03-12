use rustpython_parser::ast::{Expr, ExprKind, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[violation]
pub struct OpenFileWithContextHandler;

impl Violation for OpenFileWithContextHandler {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use context handler for opening files")
    }
}

/// Return `true` if the current expression is nested in an `await
/// exit_stack.enter_async_context` call.
fn match_async_exit_stack(checker: &Checker) -> bool {
    let Some(expr) = checker.ctx.current_expr_grandparent() else {
        return false;
    };
    let ExprKind::Await { value } = &expr.node else {
        return false;
    };
    let ExprKind::Call { func,  .. } = &value.node else {
         return false;
     };
    let ExprKind::Attribute { attr, .. } = &func.node else {
        return false;
    };
    if attr != "enter_async_context" {
        return false;
    }
    for parent in &checker.ctx.parents {
        if let StmtKind::With { items, .. } = &parent.node {
            for item in items {
                if let ExprKind::Call { func, .. } = &item.context_expr.node {
                    if checker
                        .ctx
                        .resolve_call_path(func)
                        .map_or(false, |call_path| {
                            call_path.as_slice() == ["contextlib", "AsyncExitStack"]
                        })
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Return `true` if the current expression is nested in an
/// `exit_stack.enter_context` call.
fn match_exit_stack(checker: &Checker) -> bool {
    let Some(expr) = checker.ctx.current_expr_parent() else {
        return false;
    };
    let ExprKind::Call { func,  .. } = &expr.node else {
         return false;
     };
    let ExprKind::Attribute { attr, .. } = &func.node else {
        return false;
    };
    if attr != "enter_context" {
        return false;
    }
    for parent in &checker.ctx.parents {
        if let StmtKind::With { items, .. } = &parent.node {
            for item in items {
                if let ExprKind::Call { func, .. } = &item.context_expr.node {
                    if checker
                        .ctx
                        .resolve_call_path(func)
                        .map_or(false, |call_path| {
                            call_path.as_slice() == ["contextlib", "ExitStack"]
                        })
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// SIM115
pub fn open_file_with_context_handler(checker: &mut Checker, func: &Expr) {
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["", "open"])
    {
        if checker.ctx.is_builtin("open") {
            // Ex) `with open("foo.txt") as f: ...`
            if matches!(checker.ctx.current_stmt().node, StmtKind::With { .. }) {
                return;
            }

            // Ex) `with contextlib.ExitStack() as exit_stack: ...`
            if match_exit_stack(checker) {
                return;
            }

            // Ex) `with contextlib.AsyncExitStack() as exit_stack: ...`
            if match_async_exit_stack(checker) {
                return;
            }

            checker.diagnostics.push(Diagnostic::new(
                OpenFileWithContextHandler,
                Range::from(func),
            ));
        }
    }
}
