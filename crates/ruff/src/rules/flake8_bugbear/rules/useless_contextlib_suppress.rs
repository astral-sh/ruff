use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `contextlib.suppress` without arguments.
///
/// ## Why is this bad?
/// No exceptions will be suppressed and therefore this context manager is
/// redundant. Instead, remove the context manager to improve readability.
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
/// foo()
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
            .map_or(false, |call_path| {
                matches!(call_path.as_slice(), ["contextlib", "suppress"])
            })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(UselessContextlibSuppress, expr.range()));
    }
}
