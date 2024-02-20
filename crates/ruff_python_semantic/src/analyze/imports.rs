use ruff_python_ast::helpers::map_subscript;
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

/// Returns `true` if a [`Stmt`] is an `os.environ` modification, as in:
/// ```python
/// import os
///
/// os.environ["CUDA_VISIBLE_DEVICES"] = "4"
/// ```
pub fn is_os_environ_modification(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    match stmt {
        Stmt::Expr(ast::StmtExpr { value, .. }) => match value.as_ref() {
            Expr::Call(ast::ExprCall { func, .. }) => semantic
                .resolve_call_path(func.as_ref())
                .is_some_and(|call_path| {
                    matches!(
                        call_path.as_slice(),
                        ["os", "putenv" | "unsetenv"]
                            | [
                                "os",
                                "environ",
                                "update" | "pop" | "clear" | "setdefault" | "popitem"
                            ]
                    )
                }),
            _ => false,
        },
        Stmt::Delete(ast::StmtDelete { targets, .. }) => targets.iter().any(|target| {
            semantic
                .resolve_call_path(map_subscript(target))
                .is_some_and(|call_path| matches!(call_path.as_slice(), ["os", "environ"]))
        }),
        Stmt::Assign(ast::StmtAssign { targets, .. }) => targets.iter().any(|target| {
            semantic
                .resolve_call_path(map_subscript(target))
                .is_some_and(|call_path| matches!(call_path.as_slice(), ["os", "environ"]))
        }),
        Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => semantic
            .resolve_call_path(map_subscript(target))
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["os", "environ"])),
        Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => semantic
            .resolve_call_path(map_subscript(target))
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["os", "environ"])),
        _ => false,
    }
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
