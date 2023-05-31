use rustpython_parser::ast;
use rustpython_parser::ast::Stmt;

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::prelude::Expr;
use ruff_python_ast::prelude::Ranged;
use ruff_python_semantic::analyze::visibility::{is_abstract, is_overload};

/// ## What it does
/// Checks for common errors where certain methods are annotated as having a fixed return type,
/// despite returning self at runtime. Such methods should be annotated with typing_extensions.Self
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
///
///     def __enter__(self) -> Bad:
///         ...
///
///     async def __aenter__(self) -> Bad:
///         ...
///
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
///         ...
///
///     def __enter__(self) -> Self:
///         ...
///
///     async def __aenter__(self) -> Self:
///         ...
///
///     def __iadd__(self, other: Bad) -> Self:
///         ...
/// ```
#[violation]
pub struct FixedReturnType {
    method_name: String,
}

impl Violation for FixedReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FixedReturnType { method_name } = self;
        format!("{method_name} methods on classes usually return `self` at runtime. Consider using `typing_extensions.Self` as return type")
    }
}

const SELF_RETURNING_METHODS: &[&str] = &[
    "__iadd__",
    "__isub__",
    "__ior__",
    "__iand__",
    "__imul__",
    "__itruediv__",
    "__ifloordiv__",
    "__new__",
    "__enter__",
    "__aenter__",
];

const ITERATOR_BASES: &[&[&str]] = &[
    &["typing", "Iterator"],
    &["typing", "AsyncIterator"],
    &["collections", "abc", "Iterator"],
    &["collections", "abc", "AsyncIterator"],
];

const ASYNC_ITER_RETURN_TYPES: &[&[&str]] = &[
    &["typing", "AsyncIterable"],
    &["typing", "AsyncIterator"],
    &["collections", "abc", "AsyncIterable"],
    &["collections", "abc", "AsyncIterator"],
];

const SYNC_ITER_RETURN_TYPES: &[&[&str]] = &[
    &["typing", "Iterable"],
    &["typing", "Iterator"],
    &["collections", "abc", "Iterable"],
    &["collections", "abc", "Iterator"],
];

/// PYI034
pub(crate) fn fixed_return_type(
    checker: &mut Checker,
    bases: &[Expr],
    body: &[Stmt],
    class_decorator_list: &[Expr],
) {
    // If class is final, skip
    if class_decorator_list.iter().any(|expr| {
        checker
            .semantic_model()
            .match_typing_expr(map_callable(expr), "final")
    }) {
        return;
    }

    let mut is_iter_subclass: bool = false;
    if bases.len() == 1 {
        let base = if let Expr::Subscript(ast::ExprSubscript { value, .. }) = &bases[0] {
            // Ex) class Foo(Iterator[T]):
            value
        } else {
            // Ex) class Foo(Iterator):
            &bases[0]
        };
        is_iter_subclass = checker
            .semantic_model()
            .resolve_call_path(base)
            .map_or(false, |call_path| {
                ITERATOR_BASES.contains(&call_path.as_slice())
            });
    }

    for stmt in body {
        let (
            Stmt::FunctionDef(ast::StmtFunctionDef {
                decorator_list: func_decorator_list,
                name: method_name,
                returns,
                ..
            }) | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
                decorator_list: func_decorator_list,
                name: method_name,
                returns,
                ..
            })
        ) = stmt else {
            continue;
        };

        // Skip abstractmethods and overloaded methods
        if is_abstract(checker.semantic_model(), func_decorator_list)
            || is_overload(checker.semantic_model(), func_decorator_list)
        {
            continue;
        }

        let Some(returns) = returns else {
            continue;
        };

        // Methods that should return self
        if SELF_RETURNING_METHODS.contains(&method_name.as_str()) {
            if let Expr::Name(ast::ExprName { range, .. }) = returns.as_ref() {
                if checker
                    .semantic_model()
                    .resolve_call_path(returns)
                    .map_or(false, |call_path| {
                        call_path.as_slice() == ["typing_extensions", "Self"]
                    })
                {
                    return;
                }
                checker.diagnostics.push(Diagnostic::new(
                    FixedReturnType {
                        method_name: method_name.to_string(),
                    },
                    *range,
                ));
                continue;
            }
        }

        // __iter__/__aiter__ methods that return Iterator/AsyncIterator even if class directly
        // inherits from these
        if !is_iter_subclass {
            continue;
        }

        let async_ = match method_name.as_str() {
            "__iter__" => false,
            "__aiter__" => true,
            _ => continue,
        };

        let annotation = if let Expr::Subscript(ast::ExprSubscript { value, .. }) = returns.as_ref()
        {
            // Ex) `Iterable[T]`
            value
        } else {
            // Ex) `Iterable`, `typing.Iterable`
            returns
        };

        if checker
            .semantic_model()
            .resolve_call_path(annotation)
            .map_or(false, |call_path| {
                if async_ {
                    ASYNC_ITER_RETURN_TYPES.contains(&call_path.as_slice())
                } else {
                    SYNC_ITER_RETURN_TYPES.contains(&call_path.as_slice())
                }
            })
        {
            checker.diagnostics.push(Diagnostic::new(
                FixedReturnType {
                    method_name: method_name.to_string(),
                },
                returns.range(),
            ));
        }
    }
}
