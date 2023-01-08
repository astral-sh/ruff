use rustpython_ast::Expr;

use crate::ast::helpers::{collect_call_paths, match_call_path};
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// B005
pub fn useless_contextlib_suppress(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, args: &[Expr]) {
    if match_call_path(
        &collect_call_paths(expr),
        "contextlib",
        "suppress",
        &xxxxxxxx.from_imports,
    ) && args.is_empty()
    {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::UselessContextlibSuppress,
            Range::from_located(expr),
        ));
    }
}
