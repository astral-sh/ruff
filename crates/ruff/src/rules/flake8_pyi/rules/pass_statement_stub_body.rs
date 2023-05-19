use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct PassStatementStubBody;

impl Violation for PassStatementStubBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Empty body should contain `...`, not `pass`")
    }
}

/// PYI009
pub(crate) fn pass_statement_stub_body(checker: &mut Checker, body: &[Stmt]) {
    if body.len() != 1 {
        return;
    }
    if body[0].is_pass_stmt() {
        checker
            .diagnostics
            .push(Diagnostic::new(PassStatementStubBody, body[0].range()));
    }
}
