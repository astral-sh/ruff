use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// B003
pub fn assignment_to_os_environ(checker: &mut Checker, targets: &[Expr]) {
    if targets.len() != 1 {
        return;
    }
    let target = &targets[0];
    let ExprKind::Attribute { value, attr, .. } = &target.node else {
        return;
    };
    if attr != "environ" {
        return;
    }
    let ExprKind::Name { id, .. } = &value.node else {
                    return;
                };
    if id != "os" {
        return;
    }
    checker.add_check(Check::new(
        CheckKind::AssignmentToOsEnviron,
        Range::from_located(target),
    ));
}
