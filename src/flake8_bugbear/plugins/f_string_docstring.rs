use rustpython_ast::{ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// B021
pub fn f_string_docstring(checker: &mut Checker, body: &[Stmt]) {
    let Some(stmt) = body.first() else {
        return;
    };
    let StmtKind::Expr { value } = &stmt.node else {
        return;
    };
    let ExprKind::JoinedStr { .. } = value.node else {
        return;
    };
    checker.add_check(Check::new(
        CheckKind::FStringDocstring,
        Range::from_located(stmt),
    ));
}
