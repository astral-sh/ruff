use ruff_python_ast::{self as ast, Decorator, Expr};

use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::name::{QualifiedName, UnqualifiedName};

use crate::model::SemanticModel;
use crate::{Module, ModuleSource};

#[derive(Debug, Clone, Copy, is_macro::Is)]
pub enum Visibility {
    Public,
    Private,
}

/// Returns `true` if a function is a "static method".
pub fn is_staticmethod(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list
        .iter()
        .any(|decorator| semantic.match_builtin_expr(&decorator.expression, "staticmethod"))
}

/// Returns `true` if a function is a "class method".
pub fn is_classmethod(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list
        .iter()
        .any(|decorator| semantic.match_builtin_expr(&decorator.expression, "classmethod"))
}

/// Returns `true` if a function definition is an `@overload`.
pub fn is_overload(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list
        .iter()
        .any(|decorator| semantic.match_typing_expr(&decorator.expression, "overload"))
}

/// Returns `true` if a function definition is an `@override` (PEP 698).
pub fn is_override(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list
        .iter()
        .any(|decorator| semantic.match_typing_expr(&decorator.expression, "override"))
}

/// Returns `true` if a function definition is an abstract method based on its decorators.
pub fn is_abstract(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list.iter().any(|decorator| {
        semantic
            .resolve_qualified_name(&decorator.expression)
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
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
pub fn is_property<'a, P, I>(
    decorator_list: &[Decorator],
    extra_properties: P,
    semantic: &SemanticModel,
) -> bool
where
    P: IntoIterator<IntoIter = I>,
    I: Iterator<Item = QualifiedName<'a>> + Clone,
{
    let extra_properties = extra_properties.into_iter();
    decorator_list.iter().any(|decorator| {
        semantic
            .resolve_qualified_name(map_callable(&decorator.expression))
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    ["" | "builtins" | "enum", "property"]
                        | ["functools", "cached_property"]
                        | ["abc", "abstractproperty"]
                        | ["types", "DynamicClassAttribute"]
                ) || extra_properties
                    .clone()
                    .any(|extra_property| extra_property == qualified_name)
            })
    })
}

/// Returns `true` if a function definition is an `attrs`-like validator based on its decorators.
pub fn is_validator(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list.iter().any(|decorator| {
        let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = &decorator.expression else {
            return false;
        };

        if attr.as_str() != "validator" {
            return false;
        }

        let Expr::Name(value) = value.as_ref() else {
            return false;
        };

        semantic
            .resolve_name(value)
            .is_some_and(|id| semantic.binding(id).kind.is_assignment())
    })
}

/// Returns `true` if a class is an `final`.
pub fn is_final(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list
        .iter()
        .any(|decorator| semantic.match_typing_expr(&decorator.expression, "final"))
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

/// Infer the [`Visibility`] of a module from its path.
pub(crate) fn module_visibility(module: &Module) -> Visibility {
    match &module.source {
        ModuleSource::Path(path) => {
            if path.iter().any(|m| is_private_module(m)) {
                return Visibility::Private;
            }
        }
        ModuleSource::File(path) => {
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

/// Infer the [`Visibility`] of a function from its name.
pub(crate) fn function_visibility(function: &ast::StmtFunctionDef) -> Visibility {
    if function.name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

/// Infer the [`Visibility`] of a method from its name and decorators.
pub fn method_visibility(function: &ast::StmtFunctionDef) -> Visibility {
    // Is this a setter or deleter?
    if function.decorator_list.iter().any(|decorator| {
        UnqualifiedName::from_expr(&decorator.expression).is_some_and(|name| {
            name.segments() == [function.name.as_str(), "setter"]
                || name.segments() == [function.name.as_str(), "deleter"]
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

/// Infer the [`Visibility`] of a class from its name.
pub(crate) fn class_visibility(class: &ast::StmtClassDef) -> Visibility {
    if class.name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}
