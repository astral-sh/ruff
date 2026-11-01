use ruff_python_ast::{ExprCall, ExprKind, StmtKind};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::{ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[violation_metadata(stable_since = "v0.0.219", category = Category::Suspicious)]
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
    let ExprKind::Await(await_expr) = expr.classify() else {
        return false;
    };
    let value = await_expr.value();
    let ExprKind::Call(call_node) = value.classify() else {
        return false;
    };
    let func = call_node.func();
    let ExprKind::Attribute(attribute) = func.classify() else {
        return false;
    };
    let attr = attribute.attr();
    if attr != "enter_async_context" {
        return false;
    }
    for parent in semantic.current_statements() {
        if let StmtKind::With(with_node) = parent.classify() {
            let items = with_node.items();
            for item in items {
                if let ExprKind::Call(call_node_view) = item.context_expr().classify() {
                    let func = call_node_view.func();
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
    let ExprKind::Call(call_node) = expr.classify() else {
        return false;
    };
    let func = call_node.func();
    let ExprKind::Attribute(attribute) = func.classify() else {
        return false;
    };
    let attr = attribute.attr();
    if attr != "enter_context" {
        return false;
    }
    for parent in semantic.current_statements() {
        if let StmtKind::With(with_node) = parent.classify() {
            let items = with_node.items();
            for item in items {
                if let ExprKind::Call(call_node_view) = item.context_expr().classify() {
                    let func = call_node_view.func();
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
    let ExprKind::Call(call_node) = expr.classify() else {
        return false;
    };
    let func = call_node.func();
    let ExprKind::Attribute(attribute) = func.classify() else {
        return false;
    };
    let attr = attribute.attr();
    let value = attribute.value();
    let ExprKind::Name(name_node) = value.classify() else {
        return false;
    };
    let id = name_node.id();
    matches!(
        (id.as_str(), attr.as_str()),
        ("cls", "enterClassContext") | ("self", "enterContext" | "enterAsyncContext")
    )
}

/// Return `true` if the expression is a call to `open()`,
/// or a call to some other standard-library function that opens a file.
fn is_open_call(semantic: &SemanticModel, call: ExprCall<'_>) -> bool {
    let func = call.func();

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
    let ExprKind::Attribute(attribute) = func.classify() else {
        return false;
    };
    let attr = attribute.attr();
    let value = attribute.value();

    let ExprKind::Call(call_node) = value.classify() else {
        return false;
    };
    let func = call_node.func();

    // E.g. for `pathlib.Path(...).open()`, `qualified_name_of_instance.segments() == ["pathlib", "Path"]`
    let Some(qualified_name_of_instance) = semantic.resolve_qualified_name(func) else {
        return false;
    };

    matches!(
        (qualified_name_of_instance.segments(), attr.as_str()),
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

    let ExprKind::Call(call_node) = expr.classify() else {
        return false;
    };
    let func = call_node.func();
    let arguments = call_node.arguments();

    if !arguments.is_empty() {
        return false;
    }

    let ExprKind::Attribute(attribute) = func.classify() else {
        return false;
    };
    let attr = attribute.attr();

    attr.as_str() == "close"
}

/// SIM115
pub(crate) fn open_file_with_context_handler(checker: &Checker, call: ExprCall<'_>) {
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
    if let ScopeKind::Function(stmt_function_def) = &checker.semantic().current_scope().kind
        && stmt_function_def.name() == "__enter__"
    {
        return;
    }

    checker.report_diagnostic(OpenFileWithContextHandler, call.func().range());
}
