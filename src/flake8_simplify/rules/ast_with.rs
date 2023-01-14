use rustpython_ast::{Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

fn find_nested_with(stmt: &Stmt) -> Option<&Stmt> {
    let StmtKind::With { body, .. } = &stmt.node else {
        return None
    };
    if body.len() != 1 || !matches!(body[0].node, StmtKind::With { .. }) {
        return None;
    }
    find_nested_with(&body[0]).or_else(|| Some(&body[0]))
}

/// SIM117
pub fn multiple_with_statements(checker: &mut Checker, stmt: &Stmt, parent: Option<&Stmt>) {
    if let Some(parent) = parent {
        if let StmtKind::With { body, .. } = &parent.node {
            if body.len() == 1 {
                return;
            }
        }
    }
    if let Some(with_stmt) = find_nested_with(stmt) {
        checker.diagnostics.push(Diagnostic::new(
            violations::MultipleWithStatements,
            Range::new(stmt.location, with_stmt.end_location.unwrap()),
        ));
    }
}
