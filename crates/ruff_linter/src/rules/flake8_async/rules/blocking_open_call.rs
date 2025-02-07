use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{analyze, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that async functions do not open files with blocking methods like `open`.
///
/// ## Why is this bad?
/// Blocking an async function via a blocking call will block the entire
/// event loop, preventing it from executing other tasks while waiting for the
/// call to complete, negating the benefits of asynchronous programming.
///
/// Instead of making a blocking call, use an equivalent asynchronous library
/// or function.
///
/// ## Example
/// ```python
/// async def foo():
///     with open("bar.txt") as f:
///         contents = f.read()
/// ```
///
/// Use instead:
/// ```python
/// import anyio
///
///
/// async def foo():
///     async with await anyio.open_file("bar.txt") as f:
///         contents = await f.read()
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct BlockingOpenCallInAsyncFunction;

impl Violation for BlockingOpenCallInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Async functions should not open files with blocking methods like `open`".to_string()
    }
}

/// ASYNC230
pub(crate) fn blocking_open_call(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().in_async_context() {
        return;
    }

    if is_open_call(&call.func, checker.semantic())
        || is_open_call_from_pathlib(call.func.as_ref(), checker.semantic())
    {
        checker.report_diagnostic(Diagnostic::new(
            BlockingOpenCallInAsyncFunction,
            call.func.range(),
        ));
    }
}

/// Returns `true` if the expression resolves to a blocking open call, like `open` or `Path().open()`.
fn is_open_call(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["" | "io", "open"] | ["io", "open_code"]
            )
        })
}

/// Returns `true` if an expression resolves to a call to `pathlib.Path.open`.
fn is_open_call_from_pathlib(func: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func else {
        return false;
    };

    if attr.as_str() != "open" {
        return false;
    }

    // First: is this an inlined call to `pathlib.Path.open`?
    // ```python
    // from pathlib import Path
    // Path("foo").open()
    // ```
    if let Expr::Call(call) = value.as_ref() {
        let Some(qualified_name) = semantic.resolve_qualified_name(call.func.as_ref()) else {
            return false;
        };
        if qualified_name.segments() == ["pathlib", "Path"] {
            return true;
        }
    }

    // Second, is this a call to `pathlib.Path.open` via a variable?
    // ```python
    // from pathlib import Path
    // path = Path("foo")
    // path.open()
    // ```
    let Expr::Name(name) = value.as_ref() else {
        return false;
    };

    let Some(binding_id) = semantic.resolve_name(name) else {
        return false;
    };

    let binding = semantic.binding(binding_id);

    let Some(Expr::Call(call)) = analyze::typing::find_binding_value(binding, semantic) else {
        return false;
    };

    semantic
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|qualified_name| qualified_name.segments() == ["pathlib", "Path"])
}
