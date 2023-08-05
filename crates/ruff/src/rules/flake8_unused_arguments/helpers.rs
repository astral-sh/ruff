use ruff_python_ast::{self as ast, Constant, Expr, Stmt};

use ruff_python_ast::helpers::is_docstring_stmt;

/// Return `true` if a `Stmt` is a "empty": a `pass`, `...`, `raise
/// NotImplementedError`, or `raise NotImplemented` (with or without arguments).
fn is_empty_stmt(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Pass(_) => return true,
        Stmt::Expr(ast::StmtExpr { value, range: _ }) => {
            return matches!(
                value.as_ref(),
                Expr::Constant(ast::ExprConstant {
                    value: Constant::Ellipsis,
                    ..
                })
            )
        }
        Stmt::Raise(ast::StmtRaise { exc, cause, .. }) => {
            if cause.is_none() {
                if let Some(exc) = exc {
                    match exc.as_ref() {
                        Expr::Name(ast::ExprName { id, .. }) => {
                            return id == "NotImplementedError" || id == "NotImplemented";
                        }
                        Expr::Call(ast::ExprCall { func, .. }) => {
                            if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
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
    match body {
        [] => true,
        [stmt] => is_docstring_stmt(stmt) || is_empty_stmt(stmt),
        [docstring, stmt] => is_docstring_stmt(docstring) && is_empty_stmt(stmt),
        _ => false,
    }
}
