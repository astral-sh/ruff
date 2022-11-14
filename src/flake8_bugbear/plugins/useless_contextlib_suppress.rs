use rustpython_ast::Expr;

use crate::ast::helpers::{collect_call_paths, match_call_path};
use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// B005
pub fn useless_contextlib_suppress(checker: &mut Checker, expr: &Expr, args: &[Expr]) {
    if match_call_path(
        &collect_call_paths(expr),
        "contextlib",
        "suppress",
        &checker.from_imports,
    ) && args.is_empty()
    {
        checker.add_check(Check::new(
            CheckKind::UselessContextlibSuppress,
            Range::from_located(expr),
        ));
    }
}
