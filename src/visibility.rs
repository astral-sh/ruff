//! Abstractions for tracking public and private visibility across modules,
//! classes, and functions.

use std::path::Path;

use rustpython_ast::{Expr, Stmt, StmtKind};

use crate::ast::helpers::match_module_member;
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
        match_module_member(
            expr,
            "",
            "staticmethod",
            &checker.from_imports,
            &checker.import_aliases,
        )
    })
}

/// Returns `true` if a function is a "class method".
pub fn is_classmethod(checker: &Checker, decorator_list: &[Expr]) -> bool {
    decorator_list.iter().any(|expr| {
        match_module_member(
            expr,
            "",
            "classmethod",
            &checker.from_imports,
            &checker.import_aliases,
        )
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
        match_module_member(
            expr,
            "abc",
            "abstractmethod",
            &checker.from_imports,
            &checker.import_aliases,
        )
    })
}

/// Returns `true` if a function is a "magic method".
pub fn is_magic(stmt: &Stmt) -> bool {
    match &stmt.node {
        StmtKind::FunctionDef { name, .. } | StmtKind::AsyncFunctionDef { name, .. } => {
            name.starts_with("__")
                && name.ends_with("__")
                && name != "__init__"
                && name != "__call__"
                && name != "__new__"
        }
        _ => panic!("Found non-FunctionDef in is_magic"),
    }
}

/// Returns `true` if a function is an `__init__`.
pub fn is_init(stmt: &Stmt) -> bool {
    match &stmt.node {
        StmtKind::FunctionDef { name, .. } | StmtKind::AsyncFunctionDef { name, .. } => {
            name == "__init__"
        }
        _ => panic!("Found non-FunctionDef in is_init"),
    }
}

/// Returns `true` if a module name indicates private visibility.
fn is_private_module(module_name: &str) -> bool {
    module_name.starts_with('_') || (module_name.starts_with("__") && module_name.ends_with("__"))
}

pub fn module_visibility(path: &Path) -> Visibility {
    for component in path.iter().rev() {
        if is_private_module(&component.to_string_lossy()) {
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
        StmtKind::FunctionDef { name, .. } | StmtKind::AsyncFunctionDef { name, .. } => {
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
