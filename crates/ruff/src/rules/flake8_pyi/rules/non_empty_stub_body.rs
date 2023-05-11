use rustpython_parser::ast::{self, Constant, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

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
pub(crate) fn non_empty_stub_body(checker: &mut Checker, body: &[Stmt]) {
    if body.len() != 1 {
        return;
    }
    if let StmtKind::Expr(ast::StmtExpr { value }) = &body[0].node {
        if let ExprKind::Constant(ast::ExprConstant { value, .. }) = &value.node {
            if matches!(value, Constant::Ellipsis | Constant::Str(_)) {
                return;
            }
        }
    }
    checker
        .diagnostics
        .push(Diagnostic::new(NonEmptyStubBody, body[0].range()));
}
