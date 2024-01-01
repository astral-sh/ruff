use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for simple `__iter__` methods that return `Generator`, and for
/// simple `__aiter__` methods that return `AsyncGenerator`.
///
/// ## Why is this bad?
/// Using `(Async)Iterator` for these methods is simpler and more elegant. More
/// importantly, it also reflects the fact that the precise kind of iterator
/// returned from an `__iter__` method is usually an implementation detail that
/// could change at any time. Type annotations help define a contract for a
/// function; implementation details should not leak into that contract.
///
/// For example:
/// ```python
/// from collections.abc import AsyncGenerator, Generator
/// from typing import Any
///
///
/// class CustomIterator:
///     def __iter__(self) -> Generator:
///         yield from range(42)
///
///
/// class CustomIterator2:
///     def __iter__(self) -> Generator[str, Any, None]:
///         yield from "abcdefg"
/// ```
///
/// Use instead:
/// ```python
/// from collections.abc import Iterator
///
///
/// class CustomIterator:
///     def __iter__(self) -> Iterator:
///         yield from range(42)
///
///
/// class CustomIterator2:
///     def __iter__(self) -> Iterator[str]:
///         yield from "abdefg"
/// ```
#[violation]
pub struct GeneratorReturnFromIterMethod {
    better_return_type: String,
    method_name: String,
}

impl Violation for GeneratorReturnFromIterMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GeneratorReturnFromIterMethod {
            better_return_type,
            method_name,
        } = self;
        format!("Use `{better_return_type}` as the return value for simple `{method_name}` methods")
    }
}

/// PYI058
pub(crate) fn bad_generator_return_type(
    function_def: &ast::StmtFunctionDef,
    checker: &mut Checker,
) {
    if function_def.is_async {
        return;
    }

    let name = function_def.name.as_str();

    let better_return_type = match name {
        "__iter__" => "Iterator",
        "__aiter__" => "AsyncIterator",
        _ => return,
    };

    let semantic = checker.semantic();

    if !semantic.current_scope().kind.is_class() {
        return;
    }

    let parameters = &function_def.parameters;

    if !parameters.kwonlyargs.is_empty()
        || parameters.kwarg.is_some()
        || parameters.vararg.is_some()
    {
        return;
    }

    if (parameters.args.len() + parameters.posonlyargs.len()) != 1 {
        return;
    }

    let returns = match &function_def.returns {
        Some(returns) => returns.as_ref(),
        _ => return,
    };

    if !semantic
        .resolve_call_path(map_subscript(returns))
        .is_some_and(|call_path| {
            matches!(
                (name, call_path.as_slice()),
                (
                    "__iter__",
                    ["typing" | "typing_extensions", "Generator"]
                        | ["collections", "abc", "Generator"]
                ) | (
                    "__aiter__",
                    ["typing" | "typing_extensions", "AsyncGenerator"]
                        | ["collections", "abc", "AsyncGenerator"]
                )
            )
        })
    {
        return;
    };

    // `Generator` allows three type parameters; `AsyncGenerator` allows two.
    // If type parameters are present,
    // Check that all parameters except the first one are either `typing.Any` or `None`;
    // if not, don't emit the diagnostic
    if let ast::Expr::Subscript(ast::ExprSubscript { slice, .. }) = returns {
        let ast::Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() else {
            return;
        };
        if matches!(
            (name, &elts[..]),
            ("__iter__", [_, _, _]) | ("__aiter__", [_, _])
        ) {
            if !&elts.iter().skip(1).all(|elt| is_any_or_none(elt, semantic)) {
                return;
            }
        } else {
            return;
        }
    };

    // For .py files (runtime Python!),
    // only emit the lint if it's a simple __(a)iter__ implementation
    // -- for more complex function bodies,
    // it's more likely we'll be emitting a false positive here
    if !checker.source_type.is_stub() {
        let mut yield_encountered = false;
        for stmt in &function_def.body {
            match stmt {
                ast::Stmt::Pass(_) => continue,
                ast::Stmt::Return(ast::StmtReturn { value, .. }) => {
                    if let Some(ret_val) = value {
                        if yield_encountered
                            && !matches!(ret_val.as_ref(), ast::Expr::NoneLiteral(_))
                        {
                            return;
                        }
                    }
                }
                ast::Stmt::Expr(ast::StmtExpr { value, .. }) => match value.as_ref() {
                    ast::Expr::StringLiteral(_) | ast::Expr::EllipsisLiteral(_) => continue,
                    ast::Expr::Yield(_) | ast::Expr::YieldFrom(_) => {
                        yield_encountered = true;
                        continue;
                    }
                    _ => return,
                },
                _ => return,
            }
        }
    };

    checker.diagnostics.push(Diagnostic::new(
        GeneratorReturnFromIterMethod {
            better_return_type: better_return_type.to_string(),
            method_name: name.to_string(),
        },
        function_def.identifier(),
    ));
}

fn is_any_or_none(expr: &ast::Expr, semantic: &SemanticModel) -> bool {
    semantic.match_typing_expr(expr, "Any") || matches!(expr, ast::Expr::NoneLiteral(_))
}
