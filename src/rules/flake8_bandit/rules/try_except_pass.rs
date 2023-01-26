use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, ExprKind, Located, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct TryExceptPass;
);
impl Violation for TryExceptPass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Try, Except, Pass detected.")
    }
}

/// S110
pub fn try_except_pass(
    checker: &mut Checker,
    type_: Option<&Expr>,
    _name: Option<&str>,
    body: &[Stmt],
    check_typed_exception: bool,
) {
    if body.len() == 1
        && body[0].node == StmtKind::Pass
        && (check_typed_exception
            || match &type_ {
                Some(Located {
                    node: ExprKind::Name { id, .. },
                    ..
                }) => id == "Exception",
                None => true,
                _ => false,
            })
    {
        checker.diagnostics.push(Diagnostic::new(
            TryExceptPass,
            Range::from_located(&body[0]),
        ));
    }
}
