use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::{ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for cases where files are opened (e.g., using the builtin `open()` function)
/// without using a context manager.
///
/// ## Why is this bad?
/// If a file is opened without a context manager, it is not guaranteed that
/// the file will be closed (e.g., if an exception is raised), which can cause
/// resource leaks. The rule detects a wide array of IO calls where context managers
/// could be used, such as `open`, `pathlib.Path(...).open()`, `tempfile.TemporaryFile()`
/// or`tarfile.TarFile(...).gzopen()`.
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
/// ## References
/// - [Python documentation: `open`](https://docs.python.org/3/library/functions.html#open)
#[derive(ViolationMetadata)]
pub(crate) struct OpenFileWithContextHandler;

impl Violation for OpenFileWithContextHandler {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use a context manager for opening files".to_string()
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
                    if semantic
                        .resolve_qualified_name(func)
                        .is_some_and(|qualified_name| {
                            matches!(qualified_name.segments(), ["contextlib", "AsyncExitStack"])
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
                    if semantic
                        .resolve_qualified_name(func)
                        .is_some_and(|qualified_name| {
                            matches!(qualified_name.segments(), ["contextlib", "ExitStack"])
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

/// Return `true` if the current expression is nested in a call to one of the
/// unittest context manager methods: `cls.enterClassContext()`,
/// `self.enterContext()`, or `self.enterAsyncContext()`.
fn match_unittest_context_methods(semantic: &SemanticModel) -> bool {
    let Some(expr) = semantic.current_expression_parent() else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return false;
    };
    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() else {
        return false;
    };
    let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
        return false;
    };
    matches!(
        (id.as_str(), attr.as_str()),
        ("cls", "enterClassContext") | ("self", "enterContext" | "enterAsyncContext")
    )
}

/// Return `true` if the expression is a call to `open()`,
/// or a call to some other standard-library function that opens a file.
fn is_open_call(semantic: &SemanticModel, call: &ast::ExprCall) -> bool {
    let func = &*call.func;

    // Ex) `open(...)`
    if let Some(qualified_name) = semantic.resolve_qualified_name(func) {
        return matches!(
            qualified_name.segments(),
            [
                "" | "builtins"
                    | "bz2"
                    | "codecs"
                    | "dbm"
                    | "gzip"
                    | "tarfile"
                    | "shelve"
                    | "tokenize"
                    | "wave",
                "open"
            ] | ["dbm", "gnu" | "ndbm" | "dumb" | "sqlite3", "open"]
                | ["fileinput", "FileInput" | "input"]
                | ["io", "open" | "open_code"]
                | ["lzma", "LZMAFile" | "open"]
                | ["tarfile", "TarFile", "taropen"]
                | [
                    "tempfile",
                    "TemporaryFile" | "NamedTemporaryFile" | "SpooledTemporaryFile"
                ]
        );
    }

    // Ex) `pathlib.Path(...).open()`
    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func else {
        return false;
    };

    let Expr::Call(ast::ExprCall { func, .. }) = &**value else {
        return false;
    };

    // E.g. for `pathlib.Path(...).open()`, `qualified_name_of_instance.segments() == ["pathlib", "Path"]`
    let Some(qualified_name_of_instance) = semantic.resolve_qualified_name(func) else {
        return false;
    };

    matches!(
        (qualified_name_of_instance.segments(), &**attr),
        (
            ["pathlib", "Path"] | ["zipfile", "ZipFile"] | ["lzma", "LZMAFile"],
            "open"
        ) | (
            ["tarfile", "TarFile"],
            "open" | "taropen" | "gzopen" | "bz2open" | "xzopen"
        )
    )
}

/// Return `true` if the current expression is immediately followed by a `.close()` call.
fn is_immediately_closed(semantic: &SemanticModel) -> bool {
    let Some(expr) = semantic.current_expression_grandparent() else {
        return false;
    };

    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = expr
    else {
        return false;
    };

    if !arguments.is_empty() {
        return false;
    }

    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
        return false;
    };

    attr.as_str() == "close"
}

/// SIM115
pub(crate) fn open_file_with_context_handler(checker: &Checker, call: &ast::ExprCall) {
    let semantic = checker.semantic();

    if !is_open_call(semantic, call) {
        return;
    }

    // Ex) `open("foo.txt").close()`
    if is_immediately_closed(semantic) {
        return;
    }

    // Ex) `with open("foo.txt") as f: ...`
    if semantic.current_statement().is_with_stmt() {
        return;
    }

    // Ex) `return open("foo.txt")`
    if semantic.current_statement().is_return_stmt() {
        return;
    }

    // Ex) `with contextlib.ExitStack() as exit_stack: ...`
    if match_exit_stack(semantic) {
        return;
    }

    // Ex) `with contextlib.AsyncExitStack() as exit_stack: ...`
    if match_async_exit_stack(semantic) {
        return;
    }

    // Ex) `self.enterContext(open("foo.txt"))`
    if match_unittest_context_methods(semantic) {
        return;
    }

    // Ex) `def __enter__(self): ...`
    if let ScopeKind::Function(ast::StmtFunctionDef { name, .. }) =
        &checker.semantic().current_scope().kind
    {
        if name == "__enter__" {
            return;
        }
    }

    checker.report_diagnostic(Diagnostic::new(
        OpenFileWithContextHandler,
        call.func.range(),
    ));
}
