use ruff_diagnostics::{Diagnostic, DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing::find_assigned_value;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks that async functions do not create subprocesses with blocking methods.
///
/// ## Why is this bad?
/// Blocking an async function via a blocking call will block the entire
/// event loop, preventing it from executing other tasks while waiting for the
/// call to complete, negating the benefits of asynchronous programming.
///
/// Instead of making a blocking call, use an equivalent asynchronous library
/// or function, like [`trio.run_process()`](https://trio.readthedocs.io/en/stable/reference-io.html#trio.run_process)
/// or [`anyio.run_process()`](https://anyio.readthedocs.io/en/latest/api.html#anyio.run_process).
///
/// ## Example
/// ```python
/// async def foo():
///     os.popen(cmd)
/// ```
///
/// Use instead:
/// ```python
/// async def foo():
///     asyncio.create_subprocess_shell(cmd)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct CreateSubprocessInAsyncFunction;

impl Violation for CreateSubprocessInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Async functions should not create subprocesses with blocking methods".to_string()
    }
}

/// ## What it does
/// Checks that async functions do not run processes with blocking methods.
///
/// ## Why is this bad?
/// Blocking an async function via a blocking call will block the entire
/// event loop, preventing it from executing other tasks while waiting for the
/// call to complete, negating the benefits of asynchronous programming.
///
/// Instead of making a blocking call, use an equivalent asynchronous library
/// or function, like [`trio.run_process()`](https://trio.readthedocs.io/en/stable/reference-io.html#trio.run_process)
/// or [`anyio.run_process()`](https://anyio.readthedocs.io/en/latest/api.html#anyio.run_process).
///
/// ## Example
/// ```python
/// async def foo():
///     subprocess.run(cmd)
/// ```
///
/// Use instead:
/// ```python
/// async def foo():
///     asyncio.create_subprocess_shell(cmd)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct RunProcessInAsyncFunction;

impl Violation for RunProcessInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Async functions should not run processes with blocking methods".to_string()
    }
}

/// ## What it does
/// Checks that async functions do not wait on processes with blocking methods.
///
/// ## Why is this bad?
/// Blocking an async function via a blocking call will block the entire
/// event loop, preventing it from executing other tasks while waiting for the
/// call to complete, negating the benefits of asynchronous programming.
///
/// Instead of making a blocking call, use an equivalent asynchronous library
/// or function, like [`trio.to_thread.run_sync()`](https://trio.readthedocs.io/en/latest/reference-core.html#trio.to_thread.run_sync)
/// or [`anyio.to_thread.run_sync()`](https://anyio.readthedocs.io/en/latest/api.html#anyio.to_thread.run_sync).
///
/// ## Example
/// ```python
/// async def foo():
///     os.waitpid(0)
/// ```
///
/// Use instead:
/// ```python
/// def wait_for_process():
///     os.waitpid(0)
///
///
/// async def foo():
///     await asyncio.loop.run_in_executor(None, wait_for_process)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct WaitForProcessInAsyncFunction;

impl Violation for WaitForProcessInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Async functions should not wait on processes with blocking methods".to_string()
    }
}

/// ASYNC220, ASYNC221, ASYNC222
pub(crate) fn blocking_process_invocation(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().in_async_context() {
        return;
    }

    let Some(diagnostic_kind) =
        checker
            .semantic()
            .resolve_qualified_name(call.func.as_ref())
            .and_then(|qualified_name| match qualified_name.segments() {
                ["subprocess", "Popen"] | ["os", "popen"] => {
                    Some(CreateSubprocessInAsyncFunction.into())
                }
                ["os", "system" | "posix_spawn" | "posix_spawnp"]
                | ["subprocess", "run" | "call" | "check_call" | "check_output" | "getoutput"
                | "getstatusoutput"] => Some(RunProcessInAsyncFunction.into()),
                ["os", "wait" | "wait3" | "wait4" | "waitid" | "waitpid"] => {
                    Some(WaitForProcessInAsyncFunction.into())
                }
                ["os", "spawnl" | "spawnle" | "spawnlp" | "spawnlpe" | "spawnv" | "spawnve"
                | "spawnvp" | "spawnvpe"] => {
                    if is_p_wait(call, checker.semantic()) {
                        Some(RunProcessInAsyncFunction.into())
                    } else {
                        Some(CreateSubprocessInAsyncFunction.into())
                    }
                }
                _ => None,
            })
    else {
        return;
    };
    let diagnostic = Diagnostic::new::<DiagnosticKind>(diagnostic_kind, call.func.range());
    if checker.enabled(diagnostic.kind.rule()) {
        checker.report_diagnostic(diagnostic);
    }
}

fn is_p_wait(call: &ast::ExprCall, semantic: &SemanticModel) -> bool {
    let Some(arg) = call.arguments.find_argument_value("mode", 0) else {
        return true;
    };

    if let Some(qualified_name) = semantic.resolve_qualified_name(arg) {
        return matches!(qualified_name.segments(), ["os", "P_WAIT"]);
    } else if let Expr::Name(ast::ExprName { id, .. }) = arg {
        let Some(value) = find_assigned_value(id, semantic) else {
            return false;
        };
        if let Some(qualified_name) = semantic.resolve_qualified_name(value) {
            return matches!(qualified_name.segments(), ["os", "P_WAIT"]);
        }
    }
    false
}
