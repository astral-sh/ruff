use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{Decorator, Expr, Parameters, TypeParam, TypeParams};
use ruff_python_semantic::analyze::visibility::{
    is_abstract, is_classmethod, is_new, is_overload, is_staticmethod,
};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for methods that define a custom `TypeVar` for their return type
/// annotation instead of using `typing_extensions.Self`.
///
/// ## Why is this bad?
/// If certain methods are annotated with a custom `TypeVar` type, and the
/// class is subclassed, type checkers will not be able to infer the correct
/// return type.
///
/// This check currently applies to instance methods that return `self`, class
/// methods that return an instance of `cls`, and `__new__` methods.
///
/// ## Example
/// ```python
/// class Foo:
///     def __new__(cls: type[_S], *args: str, **kwargs: int) -> _S:
///         ...
///
///     def foo(self: _S, arg: bytes) -> _S:
///         ...
///
///     @classmethod
///     def bar(cls: type[_S], arg: int) -> _S:
///         ...
/// ```
///
/// Use instead:
/// ```python
/// from typing import Self
///
///
/// class Foo:
///     def __new__(cls: type[Self], *args: str, **kwargs: int) -> Self:
///         ...
///
///     def foo(self: Self, arg: bytes) -> Self:
///         ...
///
///     @classmethod
///     def bar(cls: type[Self], arg: int) -> Self:
///         ...
/// ```
#[violation]
pub struct CustomTypeVarReturnType {
    method_name: String,
}

impl Violation for CustomTypeVarReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CustomTypeVarReturnType { method_name } = self;
        format!(
            "Methods like `{method_name}` should return `typing.Self` instead of a custom `TypeVar`"
        )
    }
}

/// PYI019
pub(crate) fn custom_type_var_return_type(
    checker: &mut Checker,
    name: &str,
    decorator_list: &[Decorator],
    returns: Option<&Expr>,
    args: &Parameters,
    type_params: Option<&TypeParams>,
) {
    // Given, e.g., `def foo(self: _S, arg: bytes) -> _T`, extract `_T`.
    let Some(return_annotation) = returns else {
        return;
    };

    // Given, e.g., `def foo(self: _S, arg: bytes)`, extract `_S`.
    let Some(self_or_cls_annotation) = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .next()
        .and_then(|parameter_with_default| parameter_with_default.parameter.annotation.as_ref())
    else {
        return;
    };

    if !checker.semantic().current_scope().kind.is_class() {
        return;
    };

    // Skip any abstract, static, and overloaded methods.
    if is_abstract(decorator_list, checker.semantic())
        || is_overload(decorator_list, checker.semantic())
        || is_staticmethod(decorator_list, checker.semantic())
    {
        return;
    }

    let uses_custom_var: bool =
        if is_classmethod(decorator_list, checker.semantic()) || is_new(name) {
            class_method(self_or_cls_annotation, return_annotation, type_params)
        } else {
            // If not static, or a class method or __new__ we know it is an instance method
            instance_method(self_or_cls_annotation, return_annotation, type_params)
        };

    if uses_custom_var {
        checker.diagnostics.push(Diagnostic::new(
            CustomTypeVarReturnType {
                method_name: name.to_string(),
            },
            return_annotation.range(),
        ));
    }
}

/// Returns `true` if the class method is annotated with a custom `TypeVar` that is likely
/// private.
fn class_method(
    cls_annotation: &Expr,
    return_annotation: &Expr,
    type_params: Option<&TypeParams>,
) -> bool {
    let Expr::Subscript(ast::ExprSubscript { slice, value, .. }) = cls_annotation else {
        return false;
    };

    let Expr::Name(value) = value.as_ref() else {
        return false;
    };

    // Don't error if the first argument is annotated with typing.Type[T].
    // These are edge cases, and it's hard to give good error messages for them.
    if value.id != "type" {
        return false;
    };

    let Expr::Name(slice) = slice.as_ref() else {
        return false;
    };

    let Expr::Name(return_annotation) = map_subscript(return_annotation) else {
        return false;
    };

    if slice.id != return_annotation.id {
        return false;
    }

    is_likely_private_typevar(&slice.id, type_params)
}

/// Returns `true` if the instance method is annotated with a custom `TypeVar` that is likely
/// private.
fn instance_method(
    self_annotation: &Expr,
    return_annotation: &Expr,
    type_params: Option<&TypeParams>,
) -> bool {
    let Expr::Name(ast::ExprName {
        id: first_arg_type, ..
    }) = self_annotation
    else {
        return false;
    };

    let Expr::Name(ast::ExprName {
        id: return_type, ..
    }) = map_subscript(return_annotation)
    else {
        return false;
    };

    if first_arg_type != return_type {
        return false;
    }

    is_likely_private_typevar(first_arg_type, type_params)
}

/// Returns `true` if the type variable is likely private.
fn is_likely_private_typevar(type_var_name: &str, type_params: Option<&TypeParams>) -> bool {
    // Ex) `_T`
    if type_var_name.starts_with('_') {
        return true;
    }
    // Ex) `class Foo[T]: ...`
    type_params.is_some_and(|type_params| {
        type_params.iter().any(|type_param| {
            if let TypeParam::TypeVar(ast::TypeParamTypeVar { name, .. }) = type_param {
                name == type_var_name
            } else {
                false
            }
        })
    })
}
