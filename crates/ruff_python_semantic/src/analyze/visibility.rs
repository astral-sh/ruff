use std::path::Path;

use ruff_python_ast::{self as ast, Decorator};

use ruff_python_ast::call_path::{collect_call_path, CallPath};
use ruff_python_ast::helpers::map_callable;

use crate::model::SemanticModel;

#[derive(Debug, Clone, Copy, is_macro::Is)]
pub enum Visibility {
    Public,
    Private,
}

/// Returns `true` if a function is a "static method".
pub fn is_staticmethod(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list.iter().any(|decorator| {
        semantic
            .resolve_call_path(map_callable(&decorator.expression))
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["", "staticmethod"]))
    })
}

/// Returns `true` if a function is a "class method".
pub fn is_classmethod(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list.iter().any(|decorator| {
        semantic
            .resolve_call_path(map_callable(&decorator.expression))
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["", "classmethod"]))
    })
}

/// Returns `true` if a function definition is an `@overload`.
pub fn is_overload(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list.iter().any(|decorator| {
        semantic.match_typing_expr(map_callable(&decorator.expression), "overload")
    })
}

/// Returns `true` if a function definition is an `@override` (PEP 698).
pub fn is_override(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list.iter().any(|decorator| {
        semantic.match_typing_expr(map_callable(&decorator.expression), "override")
    })
}

/// Returns `true` if a function definition is an abstract method based on its decorators.
pub fn is_abstract(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list.iter().any(|decorator| {
        semantic
            .resolve_call_path(map_callable(&decorator.expression))
            .is_some_and(|call_path| {
                matches!(
                    call_path.as_slice(),
                    [
                        "abc",
                        "abstractmethod"
                            | "abstractclassmethod"
                            | "abstractstaticmethod"
                            | "abstractproperty"
                    ]
                )
            })
    })
}

/// Returns `true` if a function definition is a `@property`.
/// `extra_properties` can be used to check additional non-standard
/// `@property`-like decorators.
pub fn is_property(
    decorator_list: &[Decorator],
    extra_properties: &[CallPath],
    semantic: &SemanticModel,
) -> bool {
    decorator_list.iter().any(|decorator| {
        semantic
            .resolve_call_path(map_callable(&decorator.expression))
            .is_some_and(|call_path| {
                matches!(
                    call_path.as_slice(),
                    ["", "property"] | ["functools", "cached_property"]
                ) || extra_properties
                    .iter()
                    .any(|extra_property| extra_property.as_slice() == call_path.as_slice())
            })
    })
}

/// Returns `true` if a class is an `final`.
pub fn is_final(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list
        .iter()
        .any(|decorator| semantic.match_typing_expr(map_callable(&decorator.expression), "final"))
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
    !module_name.starts_with('_') || is_magic(module_name)
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

/// A Python module can either be defined as a module path (i.e., the dot-separated path to the
/// module) or, if the module can't be resolved, as a file path (i.e., the path to the file defining
/// the module).
#[derive(Debug)]
pub enum ModuleSource<'a> {
    /// A module path is a dot-separated path to the module.
    Path(&'a [String]),
    /// A file path is the path to the file defining the module, often a script outside of a
    /// package.
    File(&'a Path),
}

impl ModuleSource<'_> {
    /// Return the `Visibility` of the module.
    pub(crate) fn to_visibility(&self) -> Visibility {
        match self {
            Self::Path(path) => {
                if path.iter().any(|m| is_private_module(m)) {
                    return Visibility::Private;
                }
            }
            Self::File(path) => {
                // Check to see if the filename itself indicates private visibility.
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
        }
        Visibility::Public
    }
}

pub(crate) fn function_visibility(function: &ast::StmtFunctionDef) -> Visibility {
    if function.name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

pub fn method_visibility(function: &ast::StmtFunctionDef) -> Visibility {
    // Is this a setter or deleter?
    if function.decorator_list.iter().any(|decorator| {
        collect_call_path(&decorator.expression).is_some_and(|call_path| {
            call_path.as_slice() == [function.name.as_str(), "setter"]
                || call_path.as_slice() == [function.name.as_str(), "deleter"]
        })
    }) {
        return Visibility::Private;
    }

    // Is the method non-private?
    if !function.name.starts_with('_') {
        return Visibility::Public;
    }

    // Is this a magic method?
    if is_magic(&function.name) {
        return Visibility::Public;
    }

    Visibility::Private
}

pub(crate) fn class_visibility(class: &ast::StmtClassDef) -> Visibility {
    if class.name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}
