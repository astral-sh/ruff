use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{ExprKind, Stmt, StmtKind};

use crate::SemanticModel;

/// Returns `true` if a [`Stmt`] is a `sys.path` modification, as in:
/// ```python
/// import sys
///
/// sys.path.append("../")
/// sys.path += ["../"]
/// ```
pub fn is_sys_path_modification(stmt: Stmt<'_>, semantic: &SemanticModel) -> bool {
    match stmt.classify() {
        StmtKind::Expr(stmt_expr) => {
            let value = stmt_expr.value();
            match value.classify() {
                ExprKind::Call(expr_call) => {
                    let func = expr_call.func();
                    semantic
                        .resolve_qualified_name(func)
                        .is_some_and(|qualified_name| {
                            matches!(
                                qualified_name.segments(),
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
                _ => false,
            }
        }
        StmtKind::AugAssign(stmt_aug_assign) => {
            let target = stmt_aug_assign.target();
            semantic
                .resolve_qualified_name(map_subscript(target))
                .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["sys", "path"]))
        }
        _ => false,
    }
}

/// Returns `true` if a [`Stmt`] is an `os.environ` modification, as in:
/// ```python
/// import os
///
/// os.environ["CUDA_VISIBLE_DEVICES"] = "4"
/// ```
pub fn is_os_environ_modification(stmt: Stmt<'_>, semantic: &SemanticModel) -> bool {
    match stmt.classify() {
        StmtKind::Expr(stmt_expr) => {
            let value = stmt_expr.value();
            match value.classify() {
                ExprKind::Call(expr_call) => {
                    let func = expr_call.func();
                    semantic
                        .resolve_qualified_name(func)
                        .is_some_and(|qualified_name| {
                            matches!(
                                qualified_name.segments(),
                                ["os", "putenv" | "unsetenv"]
                                    | [
                                        "os",
                                        "environ",
                                        "update" | "pop" | "clear" | "setdefault" | "popitem"
                                    ]
                            )
                        })
                }
                _ => false,
            }
        }
        StmtKind::Delete(stmt_delete) => {
            let targets = stmt_delete.targets();
            targets.iter().any(|target| {
                semantic
                    .resolve_qualified_name(map_subscript(target))
                    .is_some_and(|qualified_name| {
                        matches!(qualified_name.segments(), ["os", "environ"])
                    })
            })
        }
        StmtKind::Assign(stmt_assign) => {
            let targets = stmt_assign.targets();
            targets.iter().any(|target| {
                semantic
                    .resolve_qualified_name(map_subscript(target))
                    .is_some_and(|qualified_name| {
                        matches!(qualified_name.segments(), ["os", "environ"])
                    })
            })
        }
        StmtKind::AnnAssign(stmt_ann_assign) => {
            let target = stmt_ann_assign.target();
            semantic
                .resolve_qualified_name(map_subscript(target))
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["os", "environ"])
                })
        }
        StmtKind::AugAssign(stmt_aug_assign) => {
            let target = stmt_aug_assign.target();
            semantic
                .resolve_qualified_name(map_subscript(target))
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["os", "environ"])
                })
        }
        _ => false,
    }
}

/// Returns `true` if a [`Stmt`] is a `matplotlib.use` activation, as in:
/// ```python
/// import matplotlib
///
/// matplotlib.use("Agg")
/// ```
pub fn is_matplotlib_activation(stmt: Stmt<'_>, semantic: &SemanticModel) -> bool {
    let Some(expr) = stmt.as_expr_stmt() else {
        return false;
    };
    let value = expr.value();
    let Some(call_node) = value.as_call_expr() else {
        return false;
    };
    let func = call_node.func();
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["matplotlib", "use"]))
}

/// Returns `true` if a [`Stmt`] is a `pytest.importorskip()` call, as in:
/// ```python
/// import pytest
///
/// pytest.importorskip("foo.bar")
/// ```
pub fn is_pytest_importorskip(stmt: Stmt<'_>, semantic: &SemanticModel) -> bool {
    let Some(expr) = stmt.as_expr_stmt() else {
        return false;
    };
    let value = expr.value();
    let Some(call_node) = value.as_call_expr() else {
        return false;
    };
    let func = call_node.func();

    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["pytest", "importorskip"])
        })
}

/// Returns `true` if a [`Stmt`] is a dynamic modification of the Python
/// module search path, e.g.,
/// ```python
/// import site
///
/// site.addsitedir(...)
/// ```
pub fn is_site_sys_path_modification(stmt: Stmt<'_>, semantic: &SemanticModel) -> bool {
    if let Some(expr_node) = stmt.as_expr_stmt() {
        let value = expr_node.value();
        if let Some(call_node_view) = value.as_call_expr() {
            let func = call_node_view.func();
            return semantic
                .resolve_qualified_name(func)
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["site", "addsitedir"])
                });
        }
    }
    false
}
