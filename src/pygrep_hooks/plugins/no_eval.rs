use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// PGH001 - no eval
pub fn no_eval(xxxxxxxx: &mut xxxxxxxx, func: &Expr) {
    let ExprKind::Name { id, .. } = &func.node else {
        return;
    };
    if id != "eval" {
        return;
    }
    if !xxxxxxxx.is_builtin("eval") {
        return;
    }
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::NoEval,
        Range::from_located(func),
    ));
}
