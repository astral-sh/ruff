use rustpython_parser::ast::Decorator;

use ruff_python_ast::call_path::from_qualified_name;
use ruff_python_ast::helpers::map_callable;

use crate::model::SemanticModel;
use crate::scope::{Scope, ScopeKind};

const CLASS_METHODS: [&str; 3] = ["__new__", "__init_subclass__", "__class_getitem__"];
const METACLASS_BASES: [(&str, &str); 2] = [("", "type"), ("abc", "ABCMeta")];

#[derive(Copy, Clone)]
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
    let ScopeKind::Class(scope) = &scope.kind else {
        return FunctionType::Function;
    };
    if decorator_list.iter().any(|decorator| {
        // The method is decorated with a static method decorator (like
        // `@staticmethod`).
        semantic
            .resolve_call_path(map_callable(&decorator.expression))
            .map_or(false, |call_path| {
                matches!(
                    call_path.as_slice(),
                    ["", "staticmethod"] | ["abc", "abstractstaticmethod"]
                ) || staticmethod_decorators
                    .iter()
                    .any(|decorator| call_path == from_qualified_name(decorator))
            })
    }) {
        FunctionType::StaticMethod
    } else if CLASS_METHODS.contains(&name)
        // Special-case class method, like `__new__`.
        || scope.bases.iter().any(|expr| {
            // The class itself extends a known metaclass, so all methods are class methods.
            semantic.resolve_call_path(map_callable(expr)).map_or(false, |call_path| {
                METACLASS_BASES
                    .iter()
                    .any(|(module, member)| call_path.as_slice() == [*module, *member])
            })
        })
        || decorator_list.iter().any(|decorator| {
            // The method is decorated with a class method decorator (like `@classmethod`).
            semantic.resolve_call_path(map_callable(&decorator.expression)).map_or(false, |call_path| {
                matches!(call_path.as_slice(), ["", "classmethod"] | ["abc", "abstractclassmethod"]) ||
                classmethod_decorators
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
