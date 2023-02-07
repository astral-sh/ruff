use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct TryExceptPass;
);
impl Violation for TryExceptPass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`try`-`except`-`pass` detected, consider logging the exception")
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
            || type_.map_or(true, |type_| {
                checker.resolve_call_path(type_).map_or(true, |call_path| {
                    call_path.as_slice() == ["", "Exception"]
                        || call_path.as_slice() == ["", "BaseException"]
                })
            }))
    {
        checker.diagnostics.push(Diagnostic::new(
            TryExceptPass,
            Range::from_located(&body[0]),
        ));
    }
}
