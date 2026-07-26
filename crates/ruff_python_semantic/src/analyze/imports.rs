use smallvec::{SmallVec, smallvec};

use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{self as ast, Expr, Stmt};

use crate::SemanticModel;

/// Returns `true` if a [`Stmt`] is a `sys.path` modification, as in:
/// ```python
/// import sys
///
/// sys.path.append("../")
/// sys.path += ["../"]
/// ```
pub fn is_sys_path_modification(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    match stmt {
        Stmt::Expr(ast::StmtExpr {
            value,
            range: _,
            node_index: _,
        }) => match value.as_ref() {
            Expr::Call(ast::ExprCall { func, .. }) => semantic
                .resolve_qualified_name(func.as_ref())
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
                }),
            _ => false,
        },
        Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => semantic
            .resolve_qualified_name(map_subscript(target))
            .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["sys", "path"])),
        _ => false,
    }
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
                .resolve_qualified_name(func.as_ref())
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
                }),
            _ => false,
        },
        Stmt::Delete(ast::StmtDelete { targets, .. }) => targets.iter().any(|target| {
            semantic
                .resolve_qualified_name(map_subscript(target))
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["os", "environ"])
                })
        }),
        Stmt::Assign(ast::StmtAssign { targets, .. }) => targets.iter().any(|target| {
            semantic
                .resolve_qualified_name(map_subscript(target))
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["os", "environ"])
                })
        }),
        Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => semantic
            .resolve_qualified_name(map_subscript(target))
            .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["os", "environ"])),
        Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => semantic
            .resolve_qualified_name(map_subscript(target))
            .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["os", "environ"])),
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
    let Stmt::Expr(ast::StmtExpr {
        value,
        range: _,
        node_index: _,
    }) = stmt
    else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return false;
    };
    semantic
        .resolve_qualified_name(func.as_ref())
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["matplotlib", "use"]))
}

/// Returns `true` if a [`Stmt`] is a `pytest.importorskip()` call, as in:
/// ```python
/// import pytest
///
/// pytest.importorskip("foo.bar")
/// ```
pub fn is_pytest_importorskip(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return false;
    };

    semantic
        .resolve_qualified_name(func.as_ref())
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["pytest", "importorskip"])
        })
}

/// Returns `true` if a [`Stmt`] is an `assert` statement whose test is a `sys.platform` check, as
/// in:
/// ```python
/// import sys
///
/// assert sys.platform == "darwin"
///
/// import objc
/// ```
///
/// Type checkers treat such an assertion as making everything that follows it unreachable on other
/// platforms, which makes this a common idiom for guarding platform-specific imports. The
/// equivalent `if sys.platform != "darwin": raise OSError` form is already exempt, since we never
/// treat a top-level `if` block as ending the import section.
///
/// We accept the tests mypy narrows on whatever the target platform: `sys.platform == "..."`,
/// `sys.platform != "..."`, and `sys.platform.startswith("...")`, alone or combined with `and`,
/// `or`, and `not`. mypy matches these syntactically, so near-misses like `"darwin" ==
/// sys.platform`, `sys.platform in ("win32", "cygwin")`, or a `platform` name bound by
/// `from sys import platform` don't narrow and aren't exempt here either. See mypy's
/// [Python version and system platform checks].
///
/// Requiring *every* leaf of the `and`/`or`/`not` tree to be a platform check is stricter than
/// mypy, which narrows `sys.platform == "win32" and f()` when checking a non-Windows target, and a
/// linter has no target platform to reason from.
///
/// [Python version and system platform checks]: https://mypy.readthedocs.io/en/stable/common_issues.html#python-version-and-system-platform-checks
pub fn is_platform_assertion(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    let Stmt::Assert(ast::StmtAssert { test, .. }) = stmt else {
        return false;
    };

    // Walk the `and`/`or`/`not` tree the test is built from. Every leaf has to be a `sys.platform`
    // check, since anything else would leave the assertion's outcome up to something other than the
    // platform.
    let mut leaves: SmallVec<[&Expr; 4]> = smallvec![&**test];
    while let Some(expr) = leaves.pop() {
        match expr {
            // `sys.platform != "win32" and sys.platform != "cygwin"`
            Expr::BoolOp(ast::ExprBoolOp { values, .. }) => leaves.extend(values.iter()),
            // `not sys.platform == "win32"`
            Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                operand,
                ..
            }) => leaves.push(operand),
            // `sys.platform == "darwin"`
            Expr::Compare(ast::ExprCompare {
                left,
                ops,
                comparators,
                ..
            }) => {
                // mypy narrows on a single comparison only, so a chained one like
                // `sys.platform == expected == "win32"` doesn't count.
                let ([op], [right]) = (&**ops, &**comparators) else {
                    return false;
                };
                if !matches!(op, ast::CmpOp::Eq | ast::CmpOp::NotEq)
                    || !is_sys_platform(left, semantic)
                    || !right.is_string_literal_expr()
                {
                    return false;
                }
            }
            // `sys.platform.startswith("linux")`
            Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) => {
                let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
                    return false;
                };
                if attr.as_str() != "startswith" || !is_sys_platform(value, semantic) {
                    return false;
                }
                // Only a single string-literal prefix narrows. Passing a tuple of prefixes, a
                // computed argument, or the optional `start`/`end` arguments does not, and a
                // computed argument could have side effects besides.
                let [prefix] = &*arguments.args else {
                    return false;
                };
                if !arguments.keywords.is_empty() || !prefix.is_string_literal_expr() {
                    return false;
                }
            }
            _ => return false,
        }
    }

    true
}

/// Returns `true` if an [`Expr`] is spelled `sys.platform` and does refer to the `sys` module.
///
/// Only that exact spelling counts, because mypy matches the name `sys` as written: after
/// `import sys as system`, `system.platform == "darwin"` doesn't narrow, even though it reads the
/// same value.
fn is_sys_platform(expr: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Attribute(ast::ExprAttribute { value, .. }) = expr else {
        return false;
    };
    // The qualified name pins down the attribute; only the `sys` part needs matching as written.
    matches!(value.as_ref(), Expr::Name(ast::ExprName { id, .. }) if id.as_str() == "sys")
        && semantic
            .resolve_qualified_name(expr)
            .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["sys", "platform"]))
}

/// Returns `true` if a [`Stmt`] is a dynamic modification of the Python
/// module search path, e.g.,
/// ```python
/// import site
///
/// site.addsitedir(...)
/// ```
pub fn is_site_sys_path_modification(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    if let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt {
        if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
            return semantic
                .resolve_qualified_name(func.as_ref())
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["site", "addsitedir"])
                });
        }
    }
    false
}
