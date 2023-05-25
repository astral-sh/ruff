use rustpython_parser::ast::Stmt;

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;
use ruff_python_ast::helpers::is_docstring_stmt;

#[violation]
pub struct StubBodyMultipleStatements;

impl Violation for StubBodyMultipleStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function body must contain exactly 1 statement")
    }
}

/// PYI010
pub(crate) fn stub_body_multiple_statements(checker: &mut Checker, stmt: &Stmt, body: &[Stmt]) {
    match body.len() {
        1 => (),
        2 => {
            // If 2 statements and one is a docstring. Skip as this is covered by PYI021
            if is_docstring_stmt(&body[0]) {
                return;
            }
            checker.diagnostics.push(Diagnostic::new(
                StubBodyMultipleStatements,
                helpers::identifier_range(stmt, checker.locator),
            ));
        }
        _ => {
            checker.diagnostics.push(Diagnostic::new(
                StubBodyMultipleStatements,
                helpers::identifier_range(stmt, checker.locator),
            ));
        }
    }
}
