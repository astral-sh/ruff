use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{analyze, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that async functions do not contain calls to `open`, `time.sleep`,
/// or `subprocess` methods.
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
///     time.sleep(1000)
/// ```
///
/// Use instead:
/// ```python
/// async def foo():
///     await asyncio.sleep(1000)
/// ```
#[violation]
pub struct OpenSleepOrSubprocessInAsyncFunction;

impl Violation for OpenSleepOrSubprocessInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not call `open`, `time.sleep`, or `subprocess` methods")
    }
}

/// ASYNC101
pub(crate) fn open_sleep_or_subprocess_call(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().in_async_context() {
        return;
    }

    if is_open_sleep_or_subprocess_call(&call.func, checker.semantic())
        || is_open_call_from_pathlib(call.func.as_ref(), checker.semantic())
    {
        checker.diagnostics.push(Diagnostic::new(
            OpenSleepOrSubprocessInAsyncFunction,
            call.func.range(),
        ));
    }
}

/// Returns `true` if the expression resolves to a blocking call, like `time.sleep` or
/// `subprocess.run`.
fn is_open_sleep_or_subprocess_call(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(func).is_some_and(|call_path| {
        matches!(
            call_path.as_slice(),
            ["", "open"]
                | ["time", "sleep"]
                | [
                    "subprocess",
                    "run"
                        | "Popen"
                        | "call"
                        | "check_call"
                        | "check_output"
                        | "getoutput"
                        | "getstatusoutput"
                ]
                | ["os", "wait" | "wait3" | "wait4" | "waitid" | "waitpid"]
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
        let Some(call_path) = semantic.resolve_call_path(call.func.as_ref()) else {
            return false;
        };
        if call_path.as_slice() == ["pathlib", "Path"] {
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

    let Some(Expr::Call(call)) = analyze::typing::find_binding_value(&name.id, binding, semantic)
    else {
        return false;
    };

    semantic
        .resolve_call_path(call.func.as_ref())
        .is_some_and(|call_path| call_path.as_slice() == ["pathlib", "Path"])
}
