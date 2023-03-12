use rustpython_parser::ast::{Constant, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[violation]
pub struct NonEmptyStubBody;

impl Violation for NonEmptyStubBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function body must contain only `...`")
    }
}

/// PYI010
pub fn non_empty_stub_body(checker: &mut Checker, body: &[Stmt]) {
    if body.len() != 1 {
        return;
    }
    if let StmtKind::Expr { value } = &body[0].node {
        if let ExprKind::Constant { value, .. } = &value.node {
            if matches!(value, Constant::Ellipsis | Constant::Str(_)) {
                return;
            }
        }
    }
    checker
        .diagnostics
        .push(Diagnostic::new(NonEmptyStubBody, Range::from(&body[0])));
}
