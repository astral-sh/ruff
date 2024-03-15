use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing::find_assigned_value;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks that async functions do not create subprocesses with blocking methods.
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
///     os.popen(cmd)
/// ```
///
/// Use instead:
/// ```python
/// async def foo():
///     asyncio.create_subprocess_shell(cmd)
/// ```
#[violation]
pub struct CreateSubprocessInAsyncFunction;

impl Violation for CreateSubprocessInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not create subprocesses with blocking methods")
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
/// or function.
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
#[violation]
pub struct RunProcessInAsyncFunction;

impl Violation for RunProcessInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not run processes with blocking methods")
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
/// or function.
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
/// async def foo():
///     await asyncio.loop.run_in_executor(None, wait_for_process)
/// ```
#[violation]
pub struct WaitForProcessInAsyncFunction;

impl Violation for WaitForProcessInAsyncFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not wait on processes with blocking methods")
    }
}

/// ASYNC220, ASYNC221, ASYNC222
pub(crate) fn blocking_process_invocation(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().in_async_context() {
        return;
    }

    if checker.enabled(Rule::CreateSubprocessInAsyncFunction) {
        if is_subprocess_call(&call.func, checker.semantic())
            || (is_os_spawn(&call.func, checker.semantic())
                && !is_p_wait(&call, checker.semantic()))
        {
            checker.diagnostics.push(Diagnostic::new(
                CreateSubprocessInAsyncFunction,
                call.func.range(),
            ));
        }
    }

    if checker.enabled(Rule::RunProcessInAsyncFunction) {
        if is_run_process_call(&call.func, checker.semantic())
            || (is_os_spawn(&call.func, checker.semantic()) && is_p_wait(&call, checker.semantic()))
        {
            checker.diagnostics.push(Diagnostic::new(
                RunProcessInAsyncFunction,
                call.func.range(),
            ));
        }
    }

    if checker.enabled(Rule::WaitForProcessInAsyncFunction) {
        if is_wait_for_process_call(&call.func, checker.semantic()) {
            checker.diagnostics.push(Diagnostic::new(
                WaitForProcessInAsyncFunction,
                call.func.range(),
            ));
        }
    }
}

fn is_os_spawn(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                [
                    "os",
                    "spawnl"
                        | "spawnle"
                        | "spawnlp"
                        | "spawnlpe"
                        | "spawnv"
                        | "spawnve"
                        | "spawnvp"
                        | "spawnvpe"
                ]
            )
        })
}

fn is_p_wait(call: &ast::ExprCall, semantic: &SemanticModel) -> bool {
    let Some(arg) = call.arguments.find_argument("mode", 0) else {
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

fn is_subprocess_call(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["subprocess", "Popen"] | ["os", "popen"]
            )
        })
}

fn is_run_process_call(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["os", "system"]
                    | ["os", "posix_spawn"]
                    | ["os", "posix_spawnp"]
                    | [
                        "subprocess",
                        "run"
                            | "call"
                            | "check_call"
                            | "check_output"
                            | "getoutput"
                            | "getstatusoutput"
                    ]
            )
        })
}

fn is_wait_for_process_call(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["os", "wait" | "wait3" | "wait4" | "waitid" | "waitpid"] | ["time", "sleep"]
            )
        })
}
