use ruff_python_ast::call_path::collect_call_path;
use ruff_python_ast::call_path::from_qualified_name;
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::Decorator;

use crate::model::SemanticModel;
use crate::scope::{Scope, ScopeKind};

#[derive(Debug, Copy, Clone)]
pub enum FunctionType {
    Function,
    Method,
    ClassMethod,
    StaticMethod,
}

/// Classify a function based on its scope, name, and decorators.
pub fn classify(
    name: &str,
    decorator_list: &[Decorator],
    scope: &Scope,
    semantic: &SemanticModel,
    classmethod_decorators: &[String],
    staticmethod_decorators: &[String],
) -> FunctionType {
    let ScopeKind::Class(class_def) = &scope.kind else {
        return FunctionType::Function;
    };
    if decorator_list
        .iter()
        .any(|decorator| is_static_method(decorator, semantic, staticmethod_decorators))
    {
        FunctionType::StaticMethod
    } else if matches!(name, "__new__" | "__init_subclass__" | "__class_getitem__")
    // Special-case class method, like `__new__`.
        || class_def.bases().iter().any(|expr| {
            // The class itself extends a known metaclass, so all methods are class methods.
            semantic
                .resolve_call_path(map_callable(expr))
                .is_some_and( |call_path| {
                    matches!(call_path.as_slice(), ["", "type"] | ["abc", "ABCMeta"])
                })
        })
        || decorator_list.iter().any(|decorator| is_class_method(decorator, semantic, classmethod_decorators))
    {
        FunctionType::ClassMethod
    } else {
        // It's an instance method.
        FunctionType::Method
    }
}

/// Return `true` if a [`Decorator`] is indicative of a static method.
fn is_static_method(
    decorator: &Decorator,
    semantic: &SemanticModel,
    staticmethod_decorators: &[String],
) -> bool {
    let decorator = map_callable(&decorator.expression);

    // The decorator is an import, so should match against a qualified path.
    if semantic
        .resolve_call_path(decorator)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["", "staticmethod"] | ["abc", "abstractstaticmethod"]
            ) || staticmethod_decorators
                .iter()
                .any(|decorator| call_path == from_qualified_name(decorator))
        })
    {
        return true;
    }

    // We do not have a resolvable call path, most likely from a decorator like
    // `@someproperty.setter`. Instead, match on the last element.
    if !staticmethod_decorators.is_empty() {
        if collect_call_path(decorator).is_some_and(|call_path| {
            call_path.last().is_some_and(|tail| {
                staticmethod_decorators
                    .iter()
                    .any(|decorator| tail == decorator)
            })
        }) {
            return true;
        }
    }

    false
}

/// Return `true` if a [`Decorator`] is indicative of a class method.
fn is_class_method(
    decorator: &Decorator,
    semantic: &SemanticModel,
    classmethod_decorators: &[String],
) -> bool {
    let decorator = map_callable(&decorator.expression);

    // The decorator is an import, so should match against a qualified path.
    if semantic
        .resolve_call_path(decorator)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["", "classmethod"] | ["abc", "abstractclassmethod"]
            ) || classmethod_decorators
                .iter()
                .any(|decorator| call_path == from_qualified_name(decorator))
        })
    {
        return true;
    }

    // We do not have a resolvable call path, most likely from a decorator like
    // `@someproperty.setter`. Instead, match on the last element.
    if !classmethod_decorators.is_empty() {
        if collect_call_path(decorator).is_some_and(|call_path| {
            call_path.last().is_some_and(|tail| {
                classmethod_decorators
                    .iter()
                    .any(|decorator| tail == decorator)
            })
        }) {
            return true;
        }
    }

    false
}
