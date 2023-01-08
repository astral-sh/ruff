use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// B003
pub fn assignment_to_os_environ(xxxxxxxx: &mut xxxxxxxx, targets: &[Expr]) {
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
    xxxxxxxx.diagnostics.push(Diagnostic::new(
        violations::AssignmentToOsEnviron,
        Range::from_located(target),
    ));
}
