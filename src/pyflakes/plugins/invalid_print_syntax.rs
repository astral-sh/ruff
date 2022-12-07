use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::{Binding, BindingKind, Range};
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// F633
pub fn invalid_print_syntax(checker: &mut Checker, left: &Expr) {
    let ExprKind::Name { id, .. } = &left.node else {
        return;
    };
    if id != "print" {
        return;
    }
    let scope = checker.current_scope();
    let Some(Binding {
        kind: BindingKind::Builtin,
        ..
    }) = scope.values.get("print") else
    {
        return;
    };
    checker.add_check(Check::new(
        CheckKind::InvalidPrintSyntax,
        Range::from_located(left),
    ));
}
