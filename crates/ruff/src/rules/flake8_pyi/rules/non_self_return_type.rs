use rustpython_parser::ast::{self, Arguments, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{identifier_range, map_subscript};
use ruff_python_semantic::analyze::visibility::{is_abstract, is_final, is_overload};
use ruff_python_semantic::model::SemanticModel;
use ruff_python_semantic::scope::ScopeKind;

use crate::checkers::ast::Checker;

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
/// If the return type of these methods is fixed, and the class is subclassed, the type checker can
/// get confused in scenarios such as:
/// ```python
/// class Shape:
///     def set_scale(self, scale: float) -> Shape:
///         self.scale = scale
///         return self
///
/// class Circle(Shape):
///     def set_radius(self, r: float) -> Circle:
///         self.radius = r
///         return self
///
/// Circle().set_scale(0.5)  # *Shape*, not Circle
/// Circle().set_scale(0.5).set_radius(2.7)
/// # => Error: Shape has no attribute set_radius
/// ```
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
/// from typing_extensions import Self
///
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
/// ## References
/// - [PEP 673](https://peps.python.org/pep-0673/)
#[violation]
pub struct NonSelfReturnType {
    class_name: String,
    method_name: String,
}

impl Violation for NonSelfReturnType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonSelfReturnType {
            class_name,
            method_name,
        } = self;
        if matches!(class_name.as_str(), "__new__") {
            format!("`__new__` methods usually return `self` at runtime")
        } else {
            format!("`{method_name}` methods in classes like `{class_name}` usually return `self` at runtime")
        }
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Consider using `typing_extensions.Self` as return type".to_string())
    }
}

/// PYI034
pub(crate) fn non_self_return_type(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    decorator_list: &[Expr],
    returns: Option<&Expr>,
    args: &Arguments,
    async_: bool,
) {
    let ScopeKind::Class(class_def) = checker.semantic_model().scope().kind else {
        return;
    };

    if args.args.is_empty() && args.posonlyargs.is_empty() {
        return;
    }

    let Some(returns) = returns else {
        return;
    };

    // Skip any abstract or overloaded methods.
    if is_abstract(checker.semantic_model(), decorator_list)
        || is_overload(checker.semantic_model(), decorator_list)
    {
        return;
    }

    if async_ {
        if name == "__aenter__"
            && is_name(returns, &class_def.name)
            && !is_final(checker.semantic_model(), &class_def.decorator_list)
        {
            checker.diagnostics.push(Diagnostic::new(
                NonSelfReturnType {
                    class_name: class_def.name.to_string(),
                    method_name: name.to_string(),
                },
                identifier_range(stmt, checker.locator),
            ));
        }
        return;
    }

    // In-place methods that are expected to return `Self`.
    if INPLACE_BINOP_METHODS.contains(&name) {
        if !is_self(returns, checker.semantic_model()) {
            checker.diagnostics.push(Diagnostic::new(
                NonSelfReturnType {
                    class_name: class_def.name.to_string(),
                    method_name: name.to_string(),
                },
                identifier_range(stmt, checker.locator),
            ));
        }
        return;
    }

    if is_name(returns, &class_def.name) {
        if matches!(name, "__enter__" | "__new__")
            && !is_final(checker.semantic_model(), &class_def.decorator_list)
        {
            checker.diagnostics.push(Diagnostic::new(
                NonSelfReturnType {
                    class_name: class_def.name.to_string(),
                    method_name: name.to_string(),
                },
                identifier_range(stmt, checker.locator),
            ));
        }
        return;
    }

    match name {
        "__iter__" => {
            if is_iterable(returns, checker.semantic_model())
                && is_iterator(&class_def.bases, checker.semantic_model())
            {
                checker.diagnostics.push(Diagnostic::new(
                    NonSelfReturnType {
                        class_name: class_def.name.to_string(),
                        method_name: name.to_string(),
                    },
                    identifier_range(stmt, checker.locator),
                ));
            }
        }
        "__aiter__" => {
            if is_async_iterable(returns, checker.semantic_model())
                && is_async_iterator(&class_def.bases, checker.semantic_model())
            {
                checker.diagnostics.push(Diagnostic::new(
                    NonSelfReturnType {
                        class_name: class_def.name.to_string(),
                        method_name: name.to_string(),
                    },
                    identifier_range(stmt, checker.locator),
                ));
            }
        }
        _ => {}
    }
}

const INPLACE_BINOP_METHODS: &[&str] = &[
    "__iadd__",
    "__isub__",
    "__imul__",
    "__imatmul__",
    "__itruediv__",
    "__ifloordiv__",
    "__imod__",
    "__ipow__",
    "__ilshift__",
    "__irshift__",
    "__iand__",
    "__ixor__",
    "__ior__",
];

/// Return `true` if the given expression resolves to the given name.
fn is_name(expr: &Expr, name: &str) -> bool {
    let Expr::Name(ast::ExprName { id, .. }) = expr else {
        return false;
    };
    id.as_str() == name
}

/// Return `true` if the given expression resolves to `typing.Self`.
fn is_self(expr: &Expr, model: &SemanticModel) -> bool {
    model.match_typing_expr(expr, "Self")
}

/// Return `true` if the given class extends `collections.abc.Iterator`.
fn is_iterator(bases: &[Expr], model: &SemanticModel) -> bool {
    bases.iter().any(|expr| {
        model
            .resolve_call_path(map_subscript(expr))
            .map_or(false, |call_path| {
                matches!(
                    call_path.as_slice(),
                    ["typing", "Iterator"] | ["collections", "abc", "Iterator"]
                )
            })
    })
}

/// Return `true` if the given expression resolves to `collections.abc.Iterable`.
fn is_iterable(expr: &Expr, model: &SemanticModel) -> bool {
    model
        .resolve_call_path(map_subscript(expr))
        .map_or(false, |call_path| {
            matches!(
                call_path.as_slice(),
                ["typing", "Iterable" | "Iterator"]
                    | ["collections", "abc", "Iterable" | "Iterator"]
            )
        })
}

/// Return `true` if the given class extends `collections.abc.AsyncIterator`.
fn is_async_iterator(bases: &[Expr], model: &SemanticModel) -> bool {
    bases.iter().any(|expr| {
        model
            .resolve_call_path(map_subscript(expr))
            .map_or(false, |call_path| {
                matches!(
                    call_path.as_slice(),
                    ["typing", "AsyncIterator"] | ["collections", "abc", "AsyncIterator"]
                )
            })
    })
}

/// Return `true` if the given expression resolves to `collections.abc.AsyncIterable`.
fn is_async_iterable(expr: &Expr, model: &SemanticModel) -> bool {
    model
        .resolve_call_path(map_subscript(expr))
        .map_or(false, |call_path| {
            matches!(
                call_path.as_slice(),
                ["typing", "AsyncIterable" | "AsyncIterator"]
                    | ["collections", "abc", "AsyncIterable" | "AsyncIterator"]
            )
        })
}
