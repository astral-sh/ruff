use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use rustpython_ast::{Stmt, StmtKind};

/// F701
pub fn break_outside_loop<'a>(
    stmt: &'a Stmt,
    parents: &mut impl Iterator<Item = &'a Stmt>,
) -> Option<Diagnostic> {
    let mut allowed: bool = false;
    let mut child = stmt;
    for parent in parents {
        match &parent.node {
            StmtKind::For { orelse, .. }
            | StmtKind::AsyncFor { orelse, .. }
            | StmtKind::While { orelse, .. } => {
                if !orelse.contains(child) {
                    allowed = true;
                    break;
                }
            }
            StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::ClassDef { .. } => {
                break;
            }
            _ => {}
        }
        child = parent;
    }

    if allowed {
        None
    } else {
        Some(Diagnostic::new(
            violations::BreakOutsideLoop,
            Range::from_located(stmt),
        ))
    }
}
