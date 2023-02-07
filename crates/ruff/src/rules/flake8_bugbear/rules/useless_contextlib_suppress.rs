use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::Expr;

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
pub fn useless_contextlib_suppress(checker: &mut Checker, expr: &Expr, args: &[Expr]) {
    if args.is_empty()
        && checker.resolve_call_path(expr).map_or(false, |call_path| {
            call_path.as_slice() == ["contextlib", "suppress"]
        })
    {
        checker.diagnostics.push(Diagnostic::new(
            UselessContextlibSuppress,
            Range::from_located(expr),
        ));
    }
}
