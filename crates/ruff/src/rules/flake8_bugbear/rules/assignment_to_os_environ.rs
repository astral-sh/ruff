use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct AssignmentToOsEnviron;
);
impl Violation for AssignmentToOsEnviron {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assigning to `os.environ` doesn't clear the environment")
    }
}
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
    checker.diagnostics.push(Diagnostic::new(
        AssignmentToOsEnviron,
        Range::from_located(target),
    ));
}
