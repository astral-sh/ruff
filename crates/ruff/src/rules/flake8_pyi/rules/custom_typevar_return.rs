use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{Arguments, Decorator, Expr, Stmt};

// TODO: Check docs for accuracy
/// ## What it does
/// Checks if certain methods define a custom `TypeVar`s for their return annotation instead of
/// using `typing_extensions.Self`. This check currently applies for instance methods that return
/// `self`, class methods that return an instance of `cls`, and `__new__` methods.
///
/// ## Why is this bad?
/// If certain methods are annotated with a custom `TypeVar` type, and the class is subclassed,
/// type checkers will not be able to infer the correct return type.
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
pub struct CustomTypevarReturnType {
    method_name: String,
    typevar_name: String,
}

impl Violation for CustomTypevarReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CustomTypevarReturnType {
            method_name,
            typevar_name,
        } = self;
        format!("Methods like {method_name} should return `typing.Self` instead of custom typevar {typevar_name}")
    }
}

/// PYI019
pub(crate) fn custom_typevar_return_type(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    decorator_list: &[Decorator],
    returns: Option<&Expr>,
    args: &Arguments,
    async_: bool,
) {
    checker.diagnostics.push(Diagnostic::new(
        CustomTypevarReturnType {
            method_name: name.to_string(),
            typevar_name: name.to_string(),
        },
        stmt.identifier(),
    ));
}
