//! Abstractions for tracking public and private visibility across modules,
//! classes, and functions.

use std::path::Path;

use rustpython_parser::ast::{Expr, Stmt, StmtKind};

use crate::ast::helpers::collect_call_path;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Documentable;

#[derive(Debug, Clone)]
pub enum Modifier {
    Module,
    Class,
    Function,
}

#[derive(Debug, Clone)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug, Clone)]
pub struct VisibleScope {
    pub modifier: Modifier,
    pub visibility: Visibility,
}

/// Returns `true` if a function is a "static method".
pub fn is_staticmethod(checker: &Checker, decorator_list: &[Expr]) -> bool {
    decorator_list.iter().any(|expr| {
        checker.resolve_call_path(expr).map_or(false, |call_path| {
            call_path.as_slice() == ["", "staticmethod"]
        })
    })
}

/// Returns `true` if a function is a "class method".
pub fn is_classmethod(checker: &Checker, decorator_list: &[Expr]) -> bool {
    decorator_list.iter().any(|expr| {
        checker.resolve_call_path(expr).map_or(false, |call_path| {
            call_path.as_slice() == ["", "classmethod"]
        })
    })
}

/// Returns `true` if a function definition is an `@overload`.
pub fn is_overload(checker: &Checker, decorator_list: &[Expr]) -> bool {
    decorator_list
        .iter()
        .any(|expr| checker.match_typing_expr(expr, "overload"))
}

/// Returns `true` if a function definition is an `@override` (PEP 698).
pub fn is_override(checker: &Checker, decorator_list: &[Expr]) -> bool {
    decorator_list
        .iter()
        .any(|expr| checker.match_typing_expr(expr, "override"))
}

/// Returns `true` if a function definition is an `@abstractmethod`.
pub fn is_abstract(checker: &Checker, decorator_list: &[Expr]) -> bool {
    decorator_list.iter().any(|expr| {
        checker.resolve_call_path(expr).map_or(false, |call_path| {
            call_path.as_slice() == ["abc", "abstractmethod"]
                || call_path.as_slice() == ["abc", "abstractproperty"]
        })
    })
}

/// Returns `true` if a function definition is a `@property`.
pub fn is_property(checker: &Checker, decorator_list: &[Expr]) -> bool {
    decorator_list.iter().any(|expr| {
        checker.resolve_call_path(expr).map_or(false, |call_path| {
            call_path.as_slice() == ["", "property"]
                || call_path.as_slice() == ["functools", "cached_property"]
        })
    })
}
/// Returns `true` if a function is a "magic method".
pub fn is_magic(name: &str) -> bool {
    name.starts_with("__") && name.ends_with("__")
}

/// Returns `true` if a function is an `__init__`.
pub fn is_init(name: &str) -> bool {
    name == "__init__"
}

/// Returns `true` if a function is a `__new__`.
pub fn is_new(name: &str) -> bool {
    name == "__new__"
}

/// Returns `true` if a function is a `__call__`.
pub fn is_call(name: &str) -> bool {
    name == "__call__"
}

/// Returns `true` if a function is a test one.
pub fn is_test(name: &str) -> bool {
    name == "runTest" || name.starts_with("test")
}

/// Returns `true` if a module name indicates public visibility.
fn is_public_module(module_name: &str) -> bool {
    !module_name.starts_with('_') || (module_name.starts_with("__") && module_name.ends_with("__"))
}

/// Returns `true` if a module name indicates private visibility.
fn is_private_module(module_name: &str) -> bool {
    !is_public_module(module_name)
}

/// Return the stem of a module name (everything preceding the last dot).
fn stem(path: &str) -> &str {
    if let Some(index) = path.rfind('.') {
        &path[..index]
    } else {
        path
    }
}

/// Return the `Visibility` of the Python file at `Path` based on its name.
pub fn module_visibility(path: &Path) -> Visibility {
    let mut components = path.iter().rev();

    // Is the module itself private?
    // Ex) `_foo.py` (but not `__init__.py`)
    if let Some(filename) = components.next() {
        let module_name = filename.to_string_lossy();
        let module_name = stem(&module_name);
        if is_private_module(module_name) {
            return Visibility::Private;
        }
    }

    // Is the module in a private parent?
    // Ex) `_foo/bar.py`
    for component in components {
        let module_name = component.to_string_lossy();
        if is_private_module(&module_name) {
            return Visibility::Private;
        }
    }

    Visibility::Public
}

fn function_visibility(stmt: &Stmt) -> Visibility {
    match &stmt.node {
        StmtKind::FunctionDef { name, .. } | StmtKind::AsyncFunctionDef { name, .. } => {
            if name.starts_with('_') {
                Visibility::Private
            } else {
                Visibility::Public
            }
        }
        _ => panic!("Found non-FunctionDef in function_visibility"),
    }
}

fn method_visibility(stmt: &Stmt) -> Visibility {
    match &stmt.node {
        StmtKind::FunctionDef {
            name,
            decorator_list,
            ..
        }
        | StmtKind::AsyncFunctionDef {
            name,
            decorator_list,
            ..
        } => {
            // Is this a setter or deleter?
            if decorator_list.iter().any(|expr| {
                let call_path = collect_call_path(expr);
                if call_path.len() > 1 {
                    call_path[0] == name
                } else {
                    false
                }
            }) {
                return Visibility::Private;
            }

            // Is the method non-private?
            if !name.starts_with('_') {
                return Visibility::Public;
            }

            // Is this a magic method?
            if name.starts_with("__") && name.ends_with("__") {
                return Visibility::Public;
            }

            Visibility::Private
        }
        _ => panic!("Found non-FunctionDef in method_visibility"),
    }
}

fn class_visibility(stmt: &Stmt) -> Visibility {
    match &stmt.node {
        StmtKind::ClassDef { name, .. } => {
            if name.starts_with('_') {
                Visibility::Private
            } else {
                Visibility::Public
            }
        }
        _ => panic!("Found non-ClassDef in function_visibility"),
    }
}

/// Transition a `VisibleScope` based on a new `Documentable` definition.
///
/// `scope` is the current `VisibleScope`, while `Documentable` and `Stmt`
/// describe the current node used to modify visibility.
pub fn transition_scope(scope: &VisibleScope, stmt: &Stmt, kind: &Documentable) -> VisibleScope {
    match kind {
        Documentable::Function => VisibleScope {
            modifier: Modifier::Function,
            visibility: match scope {
                VisibleScope {
                    modifier: Modifier::Module,
                    visibility: Visibility::Public,
                } => function_visibility(stmt),
                VisibleScope {
                    modifier: Modifier::Class,
                    visibility: Visibility::Public,
                } => method_visibility(stmt),
                _ => Visibility::Private,
            },
        },
        Documentable::Class => VisibleScope {
            modifier: Modifier::Class,
            visibility: match scope {
                VisibleScope {
                    modifier: Modifier::Module,
                    visibility: Visibility::Public,
                } => class_visibility(stmt),
                VisibleScope {
                    modifier: Modifier::Class,
                    visibility: Visibility::Public,
                } => class_visibility(stmt),
                _ => Visibility::Private,
            },
        },
    }
}
