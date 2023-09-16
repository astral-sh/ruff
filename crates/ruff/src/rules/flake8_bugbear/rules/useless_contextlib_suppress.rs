use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `contextlib.suppress` without arguments.
///
/// ## Why is this bad?
/// `contextlib.suppress` is a context manager that suppresses exceptions. It takes,
/// as arguments, the exceptions to suppress within the enclosed block. If no
/// exceptions are specified, then the context manager won't suppress any
/// exceptions, and is thus redundant.
///
/// Consider adding exceptions to the `contextlib.suppress` call, or removing the
/// context manager entirely.
///
/// ## Example
/// ```python
/// import contextlib
///
/// with contextlib.suppress():
///     foo()
/// ```
///
/// Use instead:
/// ```python
/// import contextlib
///
/// with contextlib.suppress(Exception):
///     foo()
/// ```
///
/// ## References
/// - [Python documentation: contextlib.suppress](https://docs.python.org/3/library/contextlib.html#contextlib.suppress)
#[violation]
pub struct UselessContextlibSuppress;

impl Violation for UselessContextlibSuppress {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "No arguments passed to `contextlib.suppress`. No exceptions will be suppressed and \
             therefore this context manager is redundant"
        )
    }
}

/// B022
pub(crate) fn useless_contextlib_suppress(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    if args.is_empty()
        && checker
            .semantic()
            .resolve_call_path(func)
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["contextlib", "suppress"]))
    {
        checker
            .diagnostics
            .push(Diagnostic::new(UselessContextlibSuppress, expr.range()));
    }
}
