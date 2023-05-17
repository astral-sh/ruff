use rustpython_parser::ast::{self, Constant, Expr, Ranged, Stmt};

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
    if let Stmt::Expr(ast::StmtExpr { value, range: _ }) = &body[0] {
        if let Expr::Constant(ast::ExprConstant { value, .. }) = value.as_ref() {
            if matches!(value, Constant::Ellipsis | Constant::Str(_)) {
                return;
            }
        }
    }
    checker
        .diagnostics
        .push(Diagnostic::new(NonEmptyStubBody, body[0].range()));
}
