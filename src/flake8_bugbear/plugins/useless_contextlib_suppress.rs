use rustpython_ast::Expr;

use crate::ast::helpers::{compose_call_path, match_call_path};
use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// B005
pub fn useless_contextlib_suppress(checker: &mut Checker, expr: &Expr, args: &[Expr]) {
    if compose_call_path(expr)
        .map(|call_path| match_call_path(&call_path, "contextlib.suppress", &checker.from_imports))
        .unwrap_or(false)
        && args.is_empty()
    {
        checker.add_check(Check::new(
            CheckKind::UselessContextlibSuppress,
            Range::from_located(expr),
        ));
    }
}
