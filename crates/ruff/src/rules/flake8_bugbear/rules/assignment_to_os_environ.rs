use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct AssignmentToOsEnviron;

impl Violation for AssignmentToOsEnviron {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assigning to `os.environ` doesn't clear the environment")
    }
}
/// B003
pub(crate) fn assignment_to_os_environ(checker: &mut Checker, targets: &[Expr]) {
    if targets.len() != 1 {
        return;
    }
    let target = &targets[0];
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = target else {
        return;
    };
    if attr != "environ" {
        return;
    }
    let Expr::Name(ast::ExprName { id, .. } )= value.as_ref() else {
        return;
    };
    if id != "os" {
        return;
    }
    checker
        .diagnostics
        .push(Diagnostic::new(AssignmentToOsEnviron, target.range()));
}
