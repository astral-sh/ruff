use rustpython_parser::ast::{Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

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
pub fn pass_statement_stub_body(checker: &mut Checker, body: &[Stmt]) {
    if body.len() != 1 {
        return;
    }
    if matches!(body[0].node, StmtKind::Pass) {
        checker.diagnostics.push(Diagnostic::new(
            PassStatementStubBody,
            Range::from(&body[0]),
        ));
    }
}
