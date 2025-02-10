use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::{GeneratorKind, ScopeKind};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `await` outside `async` functions.
///
/// ## Why is this bad?
/// Using `await` outside an `async` function is a syntax error.
///
/// ## Example
/// ```python
/// import asyncio
///
///
/// def foo():
///     await asyncio.sleep(1)
/// ```
///
/// Use instead:
/// ```python
/// import asyncio
///
///
/// async def foo():
///     await asyncio.sleep(1)
/// ```
///
/// ## Notebook behavior
/// As an exception, `await` is allowed at the top level of a Jupyter notebook
/// (see: [autoawait]).
///
/// ## References
/// - [Python documentation: Await expression](https://docs.python.org/3/reference/expressions.html#await)
/// - [PEP 492: Await Expression](https://peps.python.org/pep-0492/#await-expression)
///
/// [autoawait]: https://ipython.readthedocs.io/en/stable/interactive/autoawait.html
#[derive(ViolationMetadata)]
pub(crate) struct AwaitOutsideAsync;

impl Violation for AwaitOutsideAsync {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`await` should be used within an async function".to_string()
    }
}

/// PLE1142
pub(crate) fn await_outside_async<T: Ranged>(checker: &Checker, node: T) {
    // If we're in an `async` function, we're good.
    if checker.semantic().in_async_context() {
        return;
    }

    // `await` is allowed at the top level of a Jupyter notebook.
    // See: https://ipython.readthedocs.io/en/stable/interactive/autoawait.html.
    if checker.semantic().current_scope().kind.is_module() && checker.source_type.is_ipynb() {
        return;
    }

    // Generators are evaluated lazily, so you can use `await` in them. For example:
    // ```python
    // # This is valid
    // (await x for x in y)
    // (x async for x in y)
    //
    // # This is invalid
    // (x for x in async y)
    // [await x for x in y]
    // ```
    if matches!(
        checker.semantic().current_scope().kind,
        ScopeKind::Generator(GeneratorKind::Generator)
    ) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(AwaitOutsideAsync, node.range()));
}
