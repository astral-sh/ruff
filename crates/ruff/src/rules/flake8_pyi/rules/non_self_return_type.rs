use ruff_python_ast::{self as ast, Arguments, Decorator, Expr, Parameters, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::visibility::{is_abstract, is_final, is_overload};
use ruff_python_semantic::{ScopeKind, SemanticModel};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for methods that are annotated with a fixed return type, which
/// should instead be returning `self`.
///
/// ## Why is this bad?
/// If methods like `__new__` or `__enter__` are annotated with a fixed return
/// type, and the class is subclassed, type checkers will not be able to infer
/// the correct return type.
///
/// For example:
/// ```python
/// class Shape:
///     def set_scale(self, scale: float) -> Shape:
///         self.scale = scale
///         return self
///
/// class Circle(Shape):
///     def set_radius(self, radius: float) -> Circle:
///         self.radius = radius
///         return self
///
/// # This returns `Shape`, not `Circle`.
/// Circle().set_scale(0.5)
///
/// # Thus, this expression is invalid, as `Shape` has no attribute `set_radius`.
/// Circle().set_scale(0.5).set_radius(2.7)
/// ```
///
/// Specifically, this check enforces that the return type of the following
/// methods is `Self`:
///
/// 1. In-place binary operations, like `__iadd__`, `__imul__`, etc.
/// 1. `__new__`, `__enter__`, and `__aenter__`, if those methods return the
///    class name.
/// 1. `__iter__` methods that return `Iterator`, despite the class inheriting
///    directly from `Iterator`.
/// 1. `__aiter__` methods that return `AsyncIterator`, despite the class
///    inheriting directly from `AsyncIterator`.
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
    is_async: bool,
    name: &str,
    decorator_list: &[Decorator],
    returns: Option<&Expr>,
    parameters: &Parameters,
) {
    let ScopeKind::Class(class_def) = checker.semantic().current_scope().kind else {
        return;
    };

    if parameters.args.is_empty() && parameters.posonlyargs.is_empty() {
        return;
    }

    let Some(returns) = returns else {
        return;
    };

    // Skip any abstract or overloaded methods.
    if is_abstract(decorator_list, checker.semantic())
        || is_overload(decorator_list, checker.semantic())
    {
        return;
    }

    if is_async {
        if name == "__aenter__"
            && is_name(returns, &class_def.name)
            && !is_final(&class_def.decorator_list, checker.semantic())
        {
            checker.diagnostics.push(Diagnostic::new(
                NonSelfReturnType {
                    class_name: class_def.name.to_string(),
                    method_name: name.to_string(),
                },
                stmt.identifier(),
            ));
        }
        return;
    }

    // In-place methods that are expected to return `Self`.
    if is_inplace_bin_op(name) {
        if !is_self(returns, checker.semantic()) {
            checker.diagnostics.push(Diagnostic::new(
                NonSelfReturnType {
                    class_name: class_def.name.to_string(),
                    method_name: name.to_string(),
                },
                stmt.identifier(),
            ));
        }
        return;
    }

    if is_name(returns, &class_def.name) {
        if matches!(name, "__enter__" | "__new__")
            && !is_final(&class_def.decorator_list, checker.semantic())
        {
            checker.diagnostics.push(Diagnostic::new(
                NonSelfReturnType {
                    class_name: class_def.name.to_string(),
                    method_name: name.to_string(),
                },
                stmt.identifier(),
            ));
        }
        return;
    }

    match name {
        "__iter__" => {
            if is_iterable(returns, checker.semantic())
                && is_iterator(class_def.arguments.as_deref(), checker.semantic())
            {
                checker.diagnostics.push(Diagnostic::new(
                    NonSelfReturnType {
                        class_name: class_def.name.to_string(),
                        method_name: name.to_string(),
                    },
                    stmt.identifier(),
                ));
            }
        }
        "__aiter__" => {
            if is_async_iterable(returns, checker.semantic())
                && is_async_iterator(class_def.arguments.as_deref(), checker.semantic())
            {
                checker.diagnostics.push(Diagnostic::new(
                    NonSelfReturnType {
                        class_name: class_def.name.to_string(),
                        method_name: name.to_string(),
                    },
                    stmt.identifier(),
                ));
            }
        }
        _ => {}
    }
}

/// Returns `true` if the method is an in-place binary operator.
fn is_inplace_bin_op(name: &str) -> bool {
    matches!(
        name,
        "__iadd__"
            | "__isub__"
            | "__imul__"
            | "__imatmul__"
            | "__itruediv__"
            | "__ifloordiv__"
            | "__imod__"
            | "__ipow__"
            | "__ilshift__"
            | "__irshift__"
            | "__iand__"
            | "__ixor__"
            | "__ior__"
    )
}

/// Return `true` if the given expression resolves to the given name.
fn is_name(expr: &Expr, name: &str) -> bool {
    let Expr::Name(ast::ExprName { id, .. }) = expr else {
        return false;
    };
    id.as_str() == name
}

/// Return `true` if the given expression resolves to `typing.Self`.
fn is_self(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic.match_typing_expr(expr, "Self")
}

/// Return `true` if the given class extends `collections.abc.Iterator`.
fn is_iterator(arguments: Option<&Arguments>, semantic: &SemanticModel) -> bool {
    let Some(Arguments { args: bases, .. }) = arguments else {
        return false;
    };
    bases.iter().any(|expr| {
        semantic
            .resolve_call_path(map_subscript(expr))
            .is_some_and(|call_path| {
                matches!(
                    call_path.as_slice(),
                    ["typing", "Iterator"] | ["collections", "abc", "Iterator"]
                )
            })
    })
}

/// Return `true` if the given expression resolves to `collections.abc.Iterable`.
fn is_iterable(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(map_subscript(expr))
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["typing", "Iterable" | "Iterator"]
                    | ["collections", "abc", "Iterable" | "Iterator"]
            )
        })
}

/// Return `true` if the given class extends `collections.abc.AsyncIterator`.
fn is_async_iterator(arguments: Option<&Arguments>, semantic: &SemanticModel) -> bool {
    let Some(Arguments { args: bases, .. }) = arguments else {
        return false;
    };
    bases.iter().any(|expr| {
        semantic
            .resolve_call_path(map_subscript(expr))
            .is_some_and(|call_path| {
                matches!(
                    call_path.as_slice(),
                    ["typing", "AsyncIterator"] | ["collections", "abc", "AsyncIterator"]
                )
            })
    })
}

/// Return `true` if the given expression resolves to `collections.abc.AsyncIterable`.
fn is_async_iterable(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(map_subscript(expr))
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["typing", "AsyncIterable" | "AsyncIterator"]
                    | ["collections", "abc", "AsyncIterable" | "AsyncIterator"]
            )
        })
}
