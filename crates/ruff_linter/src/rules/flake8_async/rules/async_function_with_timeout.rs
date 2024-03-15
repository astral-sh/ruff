use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `async` functions with a `timeout` argument.
///
/// ## Why is this bad?
/// Rather than implementing asynchronous timeout behavior manually, prefer
/// trio's built-in timeout functionality, available as `trio.fail_after`,
/// `trio.move_on_after`, `trio.fail_at`, and `trio.move_on_at`.
///
/// ## Known problems
/// To avoid false positives, this rule is only enabled if `trio` is imported
/// in the module.
///
/// ## Example
/// ```python
/// async def func():
///     await long_running_task(timeout=2)
/// ```
///
/// Use instead:
/// ```python
/// async def func():
///     with trio.fail_after(2):
///         await long_running_task()
/// ```
#[violation]
pub struct TrioAsyncFunctionWithTimeout;

impl Violation for TrioAsyncFunctionWithTimeout {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `trio.fail_after` and `trio.move_on_after` over manual `async` timeout behavior")
    }
}

/// TRIO109
pub(crate) fn async_function_with_timeout(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    // Detect `async` calls with a `timeout` argument.
    if !function_def.is_async {
        return;
    }

    // If `trio` isn't in scope, avoid raising the diagnostic.
    if !checker.semantic().seen_module(Modules::TRIO) {
        return;
    }

    // If the function doesn't have a `timeout` parameter, avoid raising the diagnostic.
    let Some(timeout) = function_def.parameters.find("timeout") else {
        return;
    };

    checker.diagnostics.push(Diagnostic::new(
        TrioAsyncFunctionWithTimeout,
        timeout.range(),
    ));
}
