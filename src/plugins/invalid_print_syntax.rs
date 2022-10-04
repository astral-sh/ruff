use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::{Binding, BindingKind, Range};
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

pub fn invalid_print_syntax(checker: &mut Checker, left: &Expr) {
    if let ExprKind::Name { id, .. } = &left.node {
        if id == "print" {
            let scope = checker.current_scope();
            if let Some(Binding {
                kind: BindingKind::Builtin,
                ..
            }) = scope.values.get("print")
            {
                checker.add_check(Check::new(
                    CheckKind::InvalidPrintSyntax,
                    Range::from_located(left),
                ));
            }
        }
    }
}
