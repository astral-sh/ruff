use rustpython_ast::{Stmt, StmtKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// SIM117
pub fn multiple_with_statements(xxxxxxxx: &mut xxxxxxxx, stmt: &Stmt) {
    let StmtKind::With { body, .. } = &stmt.node else {
        return;
    };
    if body.len() != 1 {
        return;
    }
    if matches!(body[0].node, StmtKind::With { .. }) {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::MultipleWithStatements,
            Range::from_located(stmt),
        ));
    }
}
