use rustpython_parser::ast::Expr;

use crate::ast::helpers::to_call_path;
use crate::ast::types::{Scope, ScopeKind};
use crate::checkers::ast::Checker;

const CLASS_METHODS: [&str; 3] = ["__new__", "__init_subclass__", "__class_getitem__"];
const METACLASS_BASES: [(&str, &str); 2] = [("", "type"), ("abc", "ABCMeta")];

pub enum FunctionType {
    Function,
    Method,
    ClassMethod,
    StaticMethod,
}

/// Classify a function based on its scope, name, and decorators.
pub fn classify(
    checker: &Checker,
    scope: &Scope,
    name: &str,
    decorator_list: &[Expr],
    classmethod_decorators: &[String],
    staticmethod_decorators: &[String],
) -> FunctionType {
    let ScopeKind::Class(scope) = &scope.kind else {
        return FunctionType::Function;
    };
    if decorator_list.iter().any(|expr| {
        // The method is decorated with a static method decorator (like
        // `@staticmethod`).
        checker.resolve_call_path(expr).map_or(false, |call_path| {
            staticmethod_decorators
                .iter()
                .any(|decorator| call_path == to_call_path(decorator))
        })
    }) {
        FunctionType::StaticMethod
    } else if CLASS_METHODS.contains(&name)
        // Special-case class method, like `__new__`.
        || scope.bases.iter().any(|expr| {
            // The class itself extends a known metaclass, so all methods are class methods.
            checker.resolve_call_path(expr).map_or(false, |call_path| {
                METACLASS_BASES
                    .iter()
                    .any(|(module, member)| call_path.as_slice() == [*module, *member])
            })
        })
        || decorator_list.iter().any(|expr| {
            // The method is decorated with a class method decorator (like `@classmethod`).
            checker.resolve_call_path(expr).map_or(false, |call_path| {
                classmethod_decorators
                    .iter()
                    .any(|decorator| call_path == to_call_path(decorator))
            })
        })
    {
        FunctionType::ClassMethod
    } else {
        // It's an instance method.
        FunctionType::Method
    }
}
