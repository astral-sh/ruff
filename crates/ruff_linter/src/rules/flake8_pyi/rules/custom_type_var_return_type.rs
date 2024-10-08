use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{Decorator, Expr, Parameters, TypeParam, TypeParams};
use ruff_python_semantic::analyze::function_type::{self, FunctionType};
use ruff_python_semantic::analyze::visibility::{is_abstract, is_overload};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for methods that define a custom `TypeVar` for their return type
/// annotation instead of using `typing_extensions.Self`.
///
/// ## Why is this bad?
/// While the semantics are often identical, using `typing_extensions.Self` is
/// more intuitive and succinct (per [PEP 673]) than a custom `TypeVar`. For
/// example, the use of `Self` will typically allow for the omission of type
/// parameters on the `self` and `cls` arguments.
///
/// This check currently applies to instance methods that return `self`, class
/// methods that return an instance of `cls`, and `__new__` methods.
///
/// ## Example
///
/// ```pyi
/// class Foo:
///     def __new__(cls: type[_S], *args: str, **kwargs: int) -> _S: ...
///     def foo(self: _S, arg: bytes) -> _S: ...
///     @classmethod
///     def bar(cls: type[_S], arg: int) -> _S: ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// from typing import Self
///
/// class Foo:
///     def __new__(cls, *args: str, **kwargs: int) -> Self: ...
///     def foo(self, arg: bytes) -> Self: ...
///     @classmethod
///     def bar(cls, arg: int) -> Self: ...
/// ```
///
/// [PEP 673]: https://peps.python.org/pep-0673/#motivation
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
    let Some(returns) = returns else {
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

    let semantic = checker.semantic();

    // Skip any abstract, static, and overloaded methods.
    if is_abstract(decorator_list, semantic) || is_overload(decorator_list, semantic) {
        return;
    }

    let method = match function_type::classify(
        name,
        decorator_list,
        semantic.current_scope(),
        semantic,
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    ) {
        FunctionType::Function => return,
        FunctionType::StaticMethod => return,
        FunctionType::ClassMethod => Method::Class(ClassMethod {
            cls_annotation: self_or_cls_annotation,
            returns,
            type_params,
        }),
        FunctionType::Method => Method::Instance(InstanceMethod {
            self_annotation: self_or_cls_annotation,
            returns,
            type_params,
        }),
    };

    if method.uses_custom_var() {
        checker.diagnostics.push(Diagnostic::new(
            CustomTypeVarReturnType {
                method_name: name.to_string(),
            },
            returns.range(),
        ));
    }
}

#[derive(Debug)]
enum Method<'a> {
    Class(ClassMethod<'a>),
    Instance(InstanceMethod<'a>),
}

impl<'a> Method<'a> {
    fn uses_custom_var(&self) -> bool {
        match self {
            Self::Class(class_method) => class_method.uses_custom_var(),
            Self::Instance(instance_method) => instance_method.uses_custom_var(),
        }
    }
}

#[derive(Debug)]
struct ClassMethod<'a> {
    cls_annotation: &'a Expr,
    returns: &'a Expr,
    type_params: Option<&'a TypeParams>,
}

impl<'a> ClassMethod<'a> {
    /// Returns `true` if the class method is annotated with a custom `TypeVar` that is likely
    /// private.
    fn uses_custom_var(&self) -> bool {
        let Expr::Subscript(ast::ExprSubscript { slice, value, .. }) = self.cls_annotation else {
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

        let Expr::Name(return_annotation) = map_subscript(self.returns) else {
            return false;
        };

        if slice.id != return_annotation.id {
            return false;
        }

        is_likely_private_typevar(&slice.id, self.type_params)
    }
}

#[derive(Debug)]
struct InstanceMethod<'a> {
    self_annotation: &'a Expr,
    returns: &'a Expr,
    type_params: Option<&'a TypeParams>,
}

impl<'a> InstanceMethod<'a> {
    /// Returns `true` if the instance method is annotated with a custom `TypeVar` that is likely
    /// private.
    fn uses_custom_var(&self) -> bool {
        let Expr::Name(ast::ExprName {
            id: first_arg_type, ..
        }) = self.self_annotation
        else {
            return false;
        };

        let Expr::Name(ast::ExprName {
            id: return_type, ..
        }) = map_subscript(self.returns)
        else {
            return false;
        };

        if first_arg_type != return_type {
            return false;
        }

        is_likely_private_typevar(first_arg_type, self.type_params)
    }
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
