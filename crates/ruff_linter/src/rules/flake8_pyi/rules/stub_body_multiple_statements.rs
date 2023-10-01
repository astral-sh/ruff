use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::Stmt;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for functions in stub files which contain multiple statements.
///
/// ## Why is this bad?
/// Function bodies in stub files should not have functional code.
/// They should be replaced with `...`
#[violation]
pub struct StubBodyMultipleStatements;

impl Violation for StubBodyMultipleStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function body must contain exactly one statement")
    }
}

/// PYI048
pub(crate) fn stub_body_multiple_statements(checker: &mut Checker, stmt: &Stmt, body: &[Stmt]) {
    if body.len() > 1 {
        checker.diagnostics.push(Diagnostic::new(
            StubBodyMultipleStatements,
            stmt.identifier(),
        ));
    }
}
