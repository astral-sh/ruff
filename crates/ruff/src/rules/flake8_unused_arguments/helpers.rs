use rustpython_parser::ast::{Constant, ExprKind, Stmt, StmtKind};

use crate::ast::helpers::is_docstring_stmt;

/// Return `true` if a `Stmt` is a "empty": a `pass`, `...`, `raise
/// NotImplementedError`, or `raise NotImplemented` (with or without arguments).
fn is_empty_stmt(stmt: &Stmt) -> bool {
    match &stmt.node {
        StmtKind::Pass => return true,
        StmtKind::Expr { value } => {
            return matches!(
                value.node,
                ExprKind::Constant {
                    value: Constant::Ellipsis,
                    ..
                }
            )
        }
        StmtKind::Raise { exc, cause } => {
            if cause.is_none() {
                if let Some(exc) = exc {
                    match &exc.node {
                        ExprKind::Name { id, .. } => {
                            return id.as_str() == "NotImplementedError"
                                || id.as_str() == "NotImplemented";
                        }
                        ExprKind::Call { func, .. } => {
                            if let ExprKind::Name { id, .. } = &func.node {
                                return id.as_str() == "NotImplementedError"
                                    || id.as_str() == "NotImplemented";
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
    false
}

pub fn is_empty(body: &[Stmt]) -> bool {
    match &body {
        [] => true,
        [stmt] => is_docstring_stmt(stmt) || is_empty_stmt(stmt),
        [docstring, stmt] => is_docstring_stmt(docstring) && is_empty_stmt(stmt),
        _ => false,
    }
}
