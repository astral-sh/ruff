use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of the builtin `open()` function without an associated context
/// manager.
///
/// ## Why is this bad?
/// If a file is opened without a context manager, it is not guaranteed that
/// the file will be closed (e.g., if an exception is raised), which can cause
/// resource leaks.
///
/// ## Example
/// ```python
/// file = open("foo.txt")
/// ...
/// file.close()
/// ```
///
/// Use instead:
/// ```python
/// with open("foo.txt") as file:
///     ...
/// ```
///
/// # References
/// - [Python documentation: `open`](https://docs.python.org/3/library/functions.html#open)
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
fn match_async_exit_stack(semantic: &SemanticModel) -> bool {
    let Some(expr) = semantic.current_expression_grandparent() else {
        return false;
    };
    let Expr::Await(ast::ExprAwait { value, range: _ }) = expr else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return false;
    };
    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
        return false;
    };
    if attr != "enter_async_context" {
        return false;
    }
    for parent in semantic.current_statements() {
        if let Stmt::With(ast::StmtWith { items, .. }) = parent {
            for item in items {
                if let Expr::Call(ast::ExprCall { func, .. }) = &item.context_expr {
                    if semantic.resolve_call_path(func).is_some_and(|call_path| {
                        matches!(call_path.as_slice(), ["contextlib", "AsyncExitStack"])
                    }) {
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
fn match_exit_stack(semantic: &SemanticModel) -> bool {
    let Some(expr) = semantic.current_expression_parent() else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return false;
    };
    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
        return false;
    };
    if attr != "enter_context" {
        return false;
    }
    for parent in semantic.current_statements() {
        if let Stmt::With(ast::StmtWith { items, .. }) = parent {
            for item in items {
                if let Expr::Call(ast::ExprCall { func, .. }) = &item.context_expr {
                    if semantic.resolve_call_path(func).is_some_and(|call_path| {
                        matches!(call_path.as_slice(), ["contextlib", "ExitStack"])
                    }) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Return `true` if `func` is the builtin `open` or `pathlib.Path(...).open`.
fn is_open(checker: &mut Checker, func: &Expr) -> bool {
    match func {
        // pathlib.Path(...).open()
        Expr::Attribute(ast::ExprAttribute { attr, value, .. }) if attr.as_str() == "open" => {
            match value.as_ref() {
                Expr::Call(ast::ExprCall { func, .. }) => checker
                    .semantic()
                    .resolve_call_path(func)
                    .is_some_and(|call_path| matches!(call_path.as_slice(), ["pathlib", "Path"])),
                _ => false,
            }
        }
        // open(...)
        Expr::Name(ast::ExprName { id, .. }) => {
            id.as_str() == "open" && checker.semantic().is_builtin("open")
        }
        _ => false,
    }
}

/// SIM115
pub(crate) fn open_file_with_context_handler(checker: &mut Checker, func: &Expr) {
    if !is_open(checker, func) {
        return;
    }

    // Ex) `with open("foo.txt") as f: ...`
    if checker.semantic().current_statement().is_with_stmt() {
        return;
    }

    // Ex) `with contextlib.ExitStack() as exit_stack: ...`
    if match_exit_stack(checker.semantic()) {
        return;
    }

    // Ex) `with contextlib.AsyncExitStack() as exit_stack: ...`
    if match_async_exit_stack(checker.semantic()) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(OpenFileWithContextHandler, func.range()));
}
