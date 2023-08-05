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
    if decorator_list.iter().any(|decorator| {
        // The method is decorated with a static method decorator (like
        // `@staticmethod`).
        semantic
            .resolve_call_path(map_callable(&decorator.expression))
            .is_some_and(|call_path| {
                matches!(
                    call_path.as_slice(),
                    ["", "staticmethod"] | ["abc", "abstractstaticmethod"]
                ) || staticmethod_decorators
                    .iter()
                    .any(|decorator| call_path == from_qualified_name(decorator))
            })
    }) {
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
        || decorator_list.iter().any(|decorator| {
            // The method is decorated with a class method decorator (like `@classmethod`).
            semantic
                .resolve_call_path(map_callable(&decorator.expression))
                .is_some_and( |call_path| {
                    matches!(
                        call_path.as_slice(),
                        ["", "classmethod"] | ["abc", "abstractclassmethod"]
                    ) || classmethod_decorators
                        .iter()
                        .any(|decorator| call_path == from_qualified_name(decorator))
                })
        })
    {
        FunctionType::ClassMethod
    } else {
        // It's an instance method.
        FunctionType::Method
    }
}
