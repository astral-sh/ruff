use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// B003
pub fn assignment_to_os_environ(checker: &mut Checker, targets: &[Expr]) {
    if targets.len() == 1 {
        let target = &targets[0];
        if let ExprKind::Attribute { value, attr, .. } = &target.node {
            if attr == "environ" {
                if let ExprKind::Name { id, .. } = &value.node {
                    if id == "os" {
                        checker.add_check(Check::new(
                            CheckKind::AssignmentToOsEnviron,
                            Range::from_located(target),
                        ));
                    }
                }
            }
        }
    }
}
