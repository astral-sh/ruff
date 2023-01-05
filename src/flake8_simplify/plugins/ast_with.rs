use rustpython_ast::{Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckKind};

/// SIM117
pub fn multiple_with_statements(checker: &mut Checker, stmt: &Stmt) {
    let StmtKind::With { body, .. } = &stmt.node else {
        return;
    };
    if body.len() != 1 {
        return;
    }
    if matches!(body[0].node, StmtKind::With { .. }) {
        checker.add_check(Check::new(
            CheckKind::MultipleWithStatements,
            Range::from_located(stmt),
        ));
    }
}
