use rustpython_parser::ast::Expr;

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

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
            .ctx
            .resolve_call_path(func)
            .map_or(false, |call_path| {
                call_path.as_slice() == ["contextlib", "suppress"]
            })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(UselessContextlibSuppress, expr.range()));
    }
}
