use ruff_python_ast::{Expr, ExprAttribute, ExprCall, ExprName, Stmt, StmtAssign};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::CallPath;
use ruff_python_semantic::SemanticModel;
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
pub(crate) fn open_sleep_or_subprocess_call(checker: &mut Checker, call: &ExprCall) {
    let should_add_to_diagnostics = if checker.semantic().in_async_context() {
        if let Some(call_path) = checker.semantic().resolve_call_path(call.func.as_ref()) {
            is_open_sleep_or_subprocess_call(&call_path)
                || is_open_call_from_pathlib(call.func.as_ref(), checker.semantic())
        } else {
            is_open_call_from_pathlib(call.func.as_ref(), checker.semantic())
        }
    } else {
        false
    };

    if !should_add_to_diagnostics {
        return;
    }
    checker.diagnostics.push(Diagnostic::new(
        OpenSleepOrSubprocessInAsyncFunction,
        call.func.range(),
    ));
}

fn is_open_sleep_or_subprocess_call(call_path: &CallPath) -> bool {
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
}

/// Analyze if the open call is from `pathlib.Path`
/// PTH123 (builtin-open) suggests to use `pathlib.Path.open` instead of `open`
fn is_open_call_from_pathlib(func: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Attribute(ExprAttribute { attr, value, .. }) = func else {
        return false;
    };

    if attr.as_str() != "open" {
        return false;
    }

    // Check first if the call is from `pathlib.Path.open`:
    // ```python
    //  from pathlib import Path
    //  Path("foo").open()
    // ```
    if let Expr::Call(call) = value.as_ref() {
        let Some(call_path) = semantic.resolve_call_path(call.func.as_ref()) else {
            return false;
        };
        if call_path.as_slice() == ["pathlib", "Path"] {
            return true;
        }
    }

    // Otherwise, check if Path.open call is bind to a variable:
    // ```python
    //  from pathlib import Path
    //  p = Path("foo")
    //  p.open()
    //
    let Expr::Name(ExprName { id, .. }) = value.as_ref() else {
        return false;
    };

    let Some(binding_id) = semantic.lookup_symbol(id) else {
        return false;
    };

    let Some(node_id) = semantic.binding(binding_id).source else {
        return false;
    };

    let Stmt::Assign(StmtAssign { value, .. }) = semantic.statement(node_id) else {
        return false;
    };

    if let Expr::Call(call) = value.as_ref() {
        let Some(call_path) = semantic.resolve_call_path(call.func.as_ref()) else {
            return false;
        };
        if call_path.as_slice() == ["pathlib", "Path"] {
            return true;
        }
    }
    false
}
