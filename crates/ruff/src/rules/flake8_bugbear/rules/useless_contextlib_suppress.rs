use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct UselessContextlibSuppress;
);
impl Violation for UselessContextlibSuppress {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "No arguments passed to `contextlib.suppress`. No exceptions will be suppressed and \
             therefore this context manager is redundant"
        )
    }
}

/// B005
pub fn useless_contextlib_suppress(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if args.is_empty()
        && checker.resolve_call_path(func).map_or(false, |call_path| {
            call_path.as_slice() == ["contextlib", "suppress"]
        })
    {
        checker.diagnostics.push(Diagnostic::new(
            UselessContextlibSuppress,
            Range::from_located(expr),
        ));
    }
}
