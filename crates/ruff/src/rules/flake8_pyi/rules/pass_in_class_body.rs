use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

use rustpython_parser::ast::{Stmt, StmtKind};

#[violation]
pub struct PassInClassBody;

impl Violation for PassInClassBody{
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"Class body must not contain "pass""#)
    }
}

/// PYI012
pub fn pass_in_class_body(checker: &mut Checker, body: &[Stmt]) {
    // Loops through all the Located in a ClassDef body and checks for
    // any nodes to be of StmtKind::Pass
    for located in body {
        if matches!(located.node, StmtKind::Pass) {
            checker.diagnostics.push(Diagnostic::new(
                 PassInClassBody,
                Range::from(located),
            ));
        }
    }
}
