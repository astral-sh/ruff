use rustpython_parser::ast::Stmt;

use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
///  Checks for common errors where certain methods are annotated as having a fixed return type,
/// despite returning self at runtime. Such methods should be annotated with _typeshed.Self.
///
/// This check looks for:
///
///   1.  Any in-place BinOp dunder methods (__iadd__, __ior__, etc.) that do not return Self.
///   2.  __new__, __enter__ and __aenter__ methods that return the class's name unparameterised.
///   3.  __iter__ methods that return Iterator, even if the class inherits directly from Iterator.
///   4.  __aiter__ methods that return AsyncIterator, even if the class inherits directly from AsyncIterator.
///
/// NOTE: This check excludes methods decorated with @overload or @AbstractMethod.
///
/// ## Why is this bad?
/// # TODO: Add
///
/// ## Example
/// ```python
/// class Foo:
///     def __new__(cls, *args: Any, **kwargs: Any) -> Bad:
///         ...
///     def __enter__(self) -> Bad:
///         ...
///     async def __aenter__(self) -> Bad:
///         ...
///     def __iadd__(self, other: Bad) -> Bad:
///         ...
/// ```
///
/// Use instead:
/// ```python
/// from _typeshed import Self
///
/// class Foo:
///     def __new__(cls, *args: Any, **kwargs: Any) -> Self:
///         ...  # Ok
///     def __enter__(self) -> Self:
///         ...
///     async def __aenter__(self) -> Self:
///         ...
///     def __iadd__(self, other: Bad) -> Self:
///         ...
/// ```
#[violation]
pub struct FixedReturnType {
    method_name: String,
    class_name: String,
    return_type: String,
}

impl Violation for FixedReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FixedReturnType {
            method_name,
            class_name,
            return_type,
        } = self;
        format!("{method_name} methods on classes like {class_name} usually return `self` at runtime. Consider using `typing_extensions.Self` instead of {return_type}")
    }
}

/// PYI034
pub(crate) fn fixed_return_type(checker: &mut Checker, stmt: &Stmt) {}
