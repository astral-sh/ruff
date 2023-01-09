use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// F633
pub fn invalid_print_syntax(checker: &mut Checker, left: &Expr) {
    let ExprKind::Name { id, .. } = &left.node else {
        return;
    };
    if id != "print" {
        return;
    }
    if !checker.is_builtin("print") {
        return;
    };
    checker.diagnostics.push(Diagnostic::new(
        violations::InvalidPrintSyntax,
        Range::from_located(left),
    ));
}
