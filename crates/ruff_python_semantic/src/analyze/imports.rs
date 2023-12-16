use ruff_python_ast::{self as ast, Expr, Stmt};

use crate::SemanticModel;

/// Returns `true` if a [`Stmt`] is a `sys.path` modification, as in:
/// ```python
/// import sys
///
/// sys.path.append("../")
/// ```
pub fn is_sys_path_modification(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    let Stmt::Expr(ast::StmtExpr { value, range: _ }) = stmt else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return false;
    };
    semantic
        .resolve_call_path(func.as_ref())
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                [
                    "sys",
                    "path",
                    "append"
                        | "insert"
                        | "extend"
                        | "remove"
                        | "pop"
                        | "clear"
                        | "reverse"
                        | "sort"
                ]
            )
        })
}

/// Returns `true` if a [`Stmt`] is a `matplotlib.use` activation, as in:
/// ```python
/// import matplotlib
///
/// matplotlib.use("Agg")
/// ```
pub fn is_matplotlib_activation(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    let Stmt::Expr(ast::StmtExpr { value, range: _ }) = stmt else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return false;
    };
    semantic
        .resolve_call_path(func.as_ref())
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["matplotlib", "use"]))
}
