use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_async::helpers::AsyncModule;
use crate::settings::types::PreviewMode;

/// ## What it does
/// Checks for `async` functions with a `timeout` argument.
///
/// ## Why is this bad?
/// Rather than implementing asynchronous timeout behavior manually, prefer
/// built-in timeout functionality, such as `asyncio.timeout`, `trio.fail_after`,
/// or `anyio.move_on_after`, among others.
///
/// ## Example
/// ```python
/// async def long_running_task(timeout):
///     ...
///
///
/// async def main():
///     await long_running_task(timeout=2)
/// ```
///
/// Use instead:
/// ```python
/// async def long_running_task():
///     ...
///
///
/// async def main():
///     with asyncio.timeout(2):
///         await long_running_task()
/// ```
///
/// [`asyncio` timeouts]: https://docs.python.org/3/library/asyncio-task.html#timeouts
/// [`anyio` timeouts]: https://anyio.readthedocs.io/en/stable/cancellation.html
/// [`trio` timeouts]: https://trio.readthedocs.io/en/stable/reference-core.html#cancellation-and-timeouts
#[violation]
pub struct AsyncFunctionWithTimeout {
    module: AsyncModule,
}

impl Violation for AsyncFunctionWithTimeout {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async function definition with a `timeout` parameter")
    }

    fn fix_title(&self) -> Option<String> {
        let Self { module } = self;
        let recommendation = match module {
            AsyncModule::AnyIo => "anyio.fail_after",
            AsyncModule::Trio => "trio.fail_after",
            AsyncModule::AsyncIo => "asyncio.timeout",
        };
        Some(format!("Use `{recommendation}` instead"))
    }
}

/// ASYNC109
pub(crate) fn async_function_with_timeout(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
) {
    // Detect `async` calls with a `timeout` argument.
    if !function_def.is_async {
        return;
    }

    // If the function doesn't have a `timeout` parameter, avoid raising the diagnostic.
    let Some(timeout) = function_def.parameters.find("timeout") else {
        return;
    };

    // Get preferred module.
    let module = if checker.semantic().seen_module(Modules::ANYIO) {
        AsyncModule::AnyIo
    } else if checker.semantic().seen_module(Modules::TRIO) {
        AsyncModule::Trio
    } else {
        AsyncModule::AsyncIo
    };

    if matches!(checker.settings.preview, PreviewMode::Disabled) {
        if matches!(module, AsyncModule::Trio) {
            checker.diagnostics.push(Diagnostic::new(
                AsyncFunctionWithTimeout { module },
                timeout.range(),
            ));
        }
    } else {
        checker.diagnostics.push(Diagnostic::new(
            AsyncFunctionWithTimeout { module },
            timeout.range(),
        ));
    }
}
