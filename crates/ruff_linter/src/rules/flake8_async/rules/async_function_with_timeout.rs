use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_async::helpers::AsyncModule;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for `async` function definitions with `timeout` parameters.
///
/// ## Why is this bad?
/// Rather than implementing asynchronous timeout behavior manually, prefer
/// built-in timeout functionality, such as `asyncio.timeout`, `trio.fail_after`,
/// or `anyio.move_on_after`, among others.
///
/// This rule is highly opinionated to enforce a design pattern
/// called ["structured concurrency"] that allows for
/// `async` functions to be oblivious to timeouts,
/// instead letting callers to handle the logic with a context manager.
///
/// ## Details
///
/// This rule attempts to detect which async framework your code is using
/// by analysing the imports in the file it's checking. If it sees an
/// `anyio` import in your code, it will assume `anyio` is your framework
/// of choice; if it sees a `trio` import, it will assume `trio`; if it
/// sees neither, it will assume `asyncio`. `asyncio.timeout` was added
/// in Python 3.11, so if `asyncio` is detected as the framework being used,
/// this rule will be ignored when your configured [`target-version`] is set
/// to less than Python 3.11.
///
/// For functions that wrap `asyncio.timeout`, `trio.fail_after` or
/// `anyio.move_on_after`, false positives from this rule can be avoided
/// by using a different parameter name.
///
/// ## Example
///
/// ```python
/// async def long_running_task(timeout): ...
///
///
/// async def main():
///     await long_running_task(timeout=2)
/// ```
///
/// Use instead:
///
/// ```python
/// async def long_running_task(): ...
///
///
/// async def main():
///     async with asyncio.timeout(2):
///         await long_running_task()
/// ```
///
/// ## References
/// - [`asyncio` timeouts](https://docs.python.org/3/library/asyncio-task.html#timeouts)
/// - [`anyio` timeouts](https://anyio.readthedocs.io/en/stable/cancellation.html)
/// - [`trio` timeouts](https://trio.readthedocs.io/en/stable/reference-core.html#cancellation-and-timeouts)
///
/// ["structured concurrency"]: https://vorpus.org/blog/some-thoughts-on-asynchronous-api-design-in-a-post-asyncawait-world/#timeouts-and-cancellation
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

    // asyncio.timeout feature was first introduced in Python 3.11
    if module == AsyncModule::AsyncIo && checker.settings.target_version < PythonVersion::Py311 {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        AsyncFunctionWithTimeout { module },
        timeout.range(),
    ));
}
