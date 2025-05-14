use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

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
