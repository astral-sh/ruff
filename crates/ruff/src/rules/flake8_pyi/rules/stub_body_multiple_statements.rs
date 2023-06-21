use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::identifier::Identifier;

use crate::checkers::ast::Checker;

#[violation]
pub struct StubBodyMultipleStatements;

impl Violation for StubBodyMultipleStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function body must contain exactly one statement")
    }
}

/// PYI010
pub(crate) fn stub_body_multiple_statements(checker: &mut Checker, stmt: &Stmt, body: &[Stmt]) {
    // If the function body consists of exactly one statement, abort.
    if body.len() == 1 {
        return;
    }

    // If the function body consists of exactly two statements, and the first is a
    // docstring, abort (this is covered by PYI021).
    if body.len() == 2 && is_docstring_stmt(&body[0]) {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        StubBodyMultipleStatements,
        stmt.identifier(),
    ));
}
