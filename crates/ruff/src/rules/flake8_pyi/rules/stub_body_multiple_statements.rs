use rustpython_parser::ast::{self, Constant, Expr, Ranged, Stmt};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct StubBodyMultipleStatements;

impl Violation for StubBodyMultipleStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function body must contain exactly 1 statement")
    }
}

/// PYI010
pub(crate) fn stub_body_multiple_statements(checker: &mut Checker, body: &[Stmt]) {
    match body.len() {
        1 => (),
        2 => {
            // If 2 statements and one is a docstring. Skip as this is covered by PYI021
            if is_docstring(&body[0]) {
                return;
            }
            checker
                .diagnostics
                .push(Diagnostic::new(StubBodyMultipleStatements, body[1].range()));
        }
        _ => {
            // Only raise violation for second of N>2 statements if 1st if not a docstring
            if !is_docstring(&body[0]) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(StubBodyMultipleStatements, body[1].range()));
            }
            for b in &body[2..] {
                checker
                    .diagnostics
                    .push(Diagnostic::new(StubBodyMultipleStatements, b.range()));
            }
        }
    }
}

fn is_docstring(stmt: &Stmt) -> bool {
    if let Stmt::Expr(ast::StmtExpr { value, range: _ }) = stmt {
        if let Expr::Constant(ast::ExprConstant { value, .. }) = value.as_ref() {
            if matches!(value, Constant::Ellipsis | Constant::Str(_)) {
                return true;
            }
        }
    }
    false
}
