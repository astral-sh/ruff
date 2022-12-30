use rustpython_ast::{ExprKind, Stmt, StmtKind};

use crate::ast::helpers;
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
        helpers::identifier_range(stmt, checker.locator),
    ));
}
