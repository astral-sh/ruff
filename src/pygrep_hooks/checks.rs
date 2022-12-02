use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

pub fn no_eval(checker: &mut Checker, func: &Expr) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "eval" {
            if checker.is_builtin("eval") {
                checker.add_check(Check::new(CheckKind::NoEval, Range::from_located(func)));
            }
        }
    }
}
