use ruff_diagnostics::{Diagnostic, AlwaysAutofixableViolation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

use rustpython_parser::ast::{Stmt, StmtKind};

#[violation]
pub struct PassInClassBody;

impl AlwaysAutofixableViolation for PassInClassBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Class body must not contain `pass`")
    }

    fn autofix_title(&self) -> String {
        format!("Remove `pass` from the class body")

    }
}

/// PYI012
pub fn pass_in_class_body(checker: &mut Checker, body: &[Stmt]) {
    // Loops through all the Located in a ClassDef body and checks for
    // any nodes to be of StmtKind::Pass
    for located in body {
        if matches!(located.node, StmtKind::Pass) {
            checker
                .diagnostics
                .push(Diagnostic::new(PassInClassBody, Range::from(located)));
        }
    }
}
