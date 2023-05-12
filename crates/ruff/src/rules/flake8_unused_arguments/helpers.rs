use rustpython_parser::ast::{self, Constant, ExprKind, Stmt, StmtKind};

use ruff_python_ast::helpers::is_docstring_stmt;

/// Return `true` if a `Stmt` is a "empty": a `pass`, `...`, `raise
/// NotImplementedError`, or `raise NotImplemented` (with or without arguments).
fn is_empty_stmt(stmt: &Stmt) -> bool {
    match &stmt.node {
        StmtKind::Pass => return true,
        StmtKind::Expr(ast::StmtExpr { value }) => {
            return matches!(
                value.node,
                ExprKind::Constant(ast::ExprConstant {
                    value: Constant::Ellipsis,
                    ..
                })
            )
        }
        StmtKind::Raise(ast::StmtRaise { exc, cause }) => {
            if cause.is_none() {
                if let Some(exc) = exc {
                    match &exc.node {
                        ExprKind::Name(ast::ExprName { id, .. }) => {
                            return id == "NotImplementedError" || id == "NotImplemented";
                        }
                        ExprKind::Call(ast::ExprCall { func, .. }) => {
                            if let ExprKind::Name(ast::ExprName { id, .. }) = &func.node {
                                return id == "NotImplementedError" || id == "NotImplemented";
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

pub(crate) fn is_empty(body: &[Stmt]) -> bool {
    match &body {
        [] => true,
        [stmt] => is_docstring_stmt(stmt) || is_empty_stmt(stmt),
        [docstring, stmt] => is_docstring_stmt(docstring) && is_empty_stmt(stmt),
        _ => false,
    }
}
