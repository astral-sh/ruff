use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// B005
pub fn useless_contextlib_suppress(checker: &mut Checker, expr: &Expr, args: &[Expr]) {
    if args.is_empty()
        && checker.resolve_call_path(expr).map_or(false, |call_path| {
            call_path.as_slice() == ["contextlib", "suppress"]
        })
    {
        checker.diagnostics.push(Diagnostic::new(
            violations::UselessContextlibSuppress,
            Range::from_located(expr),
        ));
    }
}
