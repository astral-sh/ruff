use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// F633
pub fn invalid_print_syntax(xxxxxxxx: &mut xxxxxxxx, left: &Expr) {
    let ExprKind::Name { id, .. } = &left.node else {
        return;
    };
    if id != "print" {
        return;
    }
    if !xxxxxxxx.is_builtin("print") {
        return;
    };
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::InvalidPrintSyntax,
        Range::from_located(left),
    ));
}
