use std::path::Path;

use rustpython_parser::ast::{Expr, Stmt, StmtKind};

use ruff_python_ast::call_path::{collect_call_path, CallPath};
use ruff_python_ast::helpers::map_callable;

use crate::context::Context;

#[derive(Debug, Clone, Copy)]
pub enum Modifier {
    Module,
    Class,
    Function,
}

#[derive(Debug, Clone, Copy)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug, Clone, Copy)]
pub struct VisibleScope {
    pub modifier: Modifier,
    pub visibility: Visibility,
}

/// Returns `true` if a function is a "static method".
pub fn is_staticmethod(ctx: &Context, decorator_list: &[Expr]) -> bool {
    decorator_list.iter().any(|expr| {
        ctx.resolve_call_path(map_callable(expr))
            .map_or(false, |call_path| {
                call_path.as_slice() == ["", "staticmethod"]
            })
    })
}

/// Returns `true` if a function is a "class method".
pub fn is_classmethod(ctx: &Context, decorator_list: &[Expr]) -> bool {
    decorator_list.iter().any(|expr| {
        ctx.resolve_call_path(map_callable(expr))
            .map_or(false, |call_path| {
                call_path.as_slice() == ["", "classmethod"]
            })
    })
}

/// Returns `true` if a function definition is an `@overload`.
pub fn is_overload(ctx: &Context, decorator_list: &[Expr]) -> bool {
    decorator_list
        .iter()
        .any(|expr| ctx.match_typing_expr(map_callable(expr), "overload"))
}

/// Returns `true` if a function definition is an `@override` (PEP 698).
pub fn is_override(ctx: &Context, decorator_list: &[Expr]) -> bool {
    decorator_list
        .iter()
        .any(|expr| ctx.match_typing_expr(map_callable(expr), "override"))
}

/// Returns `true` if a function definition is an `@abstractmethod`.
pub fn is_abstract(ctx: &Context, decorator_list: &[Expr]) -> bool {
    decorator_list.iter().any(|expr| {
        ctx.resolve_call_path(map_callable(expr))
            .map_or(false, |call_path| {
                call_path.as_slice() == ["abc", "abstractmethod"]
                    || call_path.as_slice() == ["abc", "abstractproperty"]
            })
    })
}

/// Returns `true` if a function definition is a `@property`.
/// `extra_properties` can be used to check additional non-standard
/// `@property`-like decorators.
pub fn is_property(ctx: &Context, decorator_list: &[Expr], extra_properties: &[CallPath]) -> bool {
    decorator_list.iter().any(|expr| {
        ctx.resolve_call_path(map_callable(expr))
            .map_or(false, |call_path| {
                call_path.as_slice() == ["", "property"]
                    || call_path.as_slice() == ["functools", "cached_property"]
                    || extra_properties
                        .iter()
                        .any(|extra_property| extra_property.as_slice() == call_path.as_slice())
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
pub fn module_visibility(module_path: Option<&[String]>, path: &Path) -> Visibility {
    if let Some(module_path) = module_path {
        if module_path.iter().any(|m| is_private_module(m)) {
            return Visibility::Private;
        }
    } else {
        // When module_path is None, path is a script outside a package, so just
        // check to see if the module name itself is private.
        // Ex) `_foo.py` (but not `__init__.py`)
        let mut components = path.iter().rev();
        if let Some(filename) = components.next() {
            let module_name = filename.to_string_lossy();
            let module_name = stem(&module_name);
            if is_private_module(module_name) {
                return Visibility::Private;
            }
        }
    }
    Visibility::Public
}

pub fn function_visibility(stmt: &Stmt) -> Visibility {
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

pub fn method_visibility(stmt: &Stmt) -> Visibility {
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
                collect_call_path(expr).map_or(false, |call_path| {
                    call_path.as_slice() == [name, "setter"]
                        || call_path.as_slice() == [name, "deleter"]
                })
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

pub fn class_visibility(stmt: &Stmt) -> Visibility {
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
