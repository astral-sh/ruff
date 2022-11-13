use rustpython_ast::{ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// B021
pub fn f_string_docstring(checker: &mut Checker, body: &[Stmt]) {
    if let Some(stmt) = body.first() {
        if let StmtKind::Expr { value } = &stmt.node {
            if let ExprKind::JoinedStr { .. } = value.node {
                checker.add_check(Check::new(
                    CheckKind::FStringDocstring,
                    Range::from_located(stmt),
                ));
            }
        }
    }
}
