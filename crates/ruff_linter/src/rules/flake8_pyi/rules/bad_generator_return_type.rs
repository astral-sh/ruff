use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::TextRange;

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
///
/// ## Fix safety
/// This rule tries hard to avoid false-positive errors, and the rule's fix
/// should always be safe for `.pyi` stub files. However, there is a slightly
/// higher chance that a false positive might be emitted by this rule when
/// applied to runtime Python (`.py` files). As such, the fix is marked as
/// unsafe for any `__iter__` or `__aiter__` method in a `.py` file that has
/// more than two statements (including docstrings) in its body.
#[violation]
pub struct GeneratorReturnFromIterMethod {
    better_return_type: String,
    method_name: String,
}

impl Violation for GeneratorReturnFromIterMethod {
    // Fixable iff the fully qualified name is being used:
    // one of {typing.Generator, typing_extensions.Generator, collections.abc.Generator}
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let GeneratorReturnFromIterMethod {
            better_return_type,
            method_name,
        } = self;
        format!("Use `{better_return_type}` as the return value for simple `{method_name}` methods")
    }

    fn fix_title(&self) -> Option<String> {
        let GeneratorReturnFromIterMethod {
            better_return_type,
            method_name,
        } = self;
        Some(format!(
            "Convert the return annotation of your `{method_name}` method to `{better_return_type}`"
        ))
    }
}

struct YieldTypeInfo {
    expr: ast::Expr,
    range: TextRange,
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
    // check that all parameters except the first one are either `typing.Any` or `None`:
    // - if so, collect information on the first parameter for use in the rule's autofix;
    // - if not, don't emit the diagnostic
    let yield_type_info = match returns {
        ast::Expr::Subscript(ast::ExprSubscript { slice, .. }) => match slice.as_ref() {
            ast::Expr::Tuple(slice_tuple @ ast::ExprTuple { .. }) => {
                if !&slice_tuple
                    .elts
                    .iter()
                    .skip(1)
                    .all(|elt| is_any_or_none(elt, semantic))
                {
                    return;
                }
                let yield_type = match (name, &slice_tuple.elts[..]) {
                    ("__iter__", [yield_type, _, _]) => yield_type,
                    ("__aiter__", [yield_type, _]) => yield_type,
                    _ => return,
                };
                Some(YieldTypeInfo {
                    expr: yield_type.to_owned(),
                    range: slice_tuple.range,
                })
            }
            _ => return,
        },
        _ => None,
    };

    // For .py files (runtime Python!),
    // only emit the lint if it's a simple __(a)iter__ implementation
    // -- for more complex function bodies,
    // it's more likely we'll be emitting a false positive here
    let is_stub = checker.source_type.is_stub();
    if !is_stub {
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
    let mut diagnostic = Diagnostic::new(
        GeneratorReturnFromIterMethod {
            better_return_type: better_return_type.to_string(),
            method_name: name.to_string(),
        },
        function_def.identifier(),
    );
    if let Some(fix) = get_fix(function_def, checker, returns, is_stub, yield_type_info) {
        diagnostic.set_fix(fix);
    };
    checker.diagnostics.push(diagnostic);
}

fn is_any_or_none(expr: &ast::Expr, semantic: &SemanticModel) -> bool {
    semantic.match_typing_expr(expr, "Any") || matches!(expr, ast::Expr::NoneLiteral(_))
}

fn get_fix(
    function_def: &ast::StmtFunctionDef,
    checker: &Checker,
    returns: &ast::Expr,
    is_stub: bool,
    yield_type_info: Option<YieldTypeInfo>,
) -> Option<Fix> {
    let edit = match returns {
        ast::Expr::Attribute(_) => get_edit(returns),
        ast::Expr::Subscript(ast::ExprSubscript { value, .. }) => get_edit(value.as_ref()),
        _ => None,
    };

    let Some(edit) = edit else {
        return None;
    };
    let mut rest = vec![];
    if let Some(yield_type_info) = yield_type_info {
        rest.push(Edit::range_replacement(
            checker.generator().expr(&yield_type_info.expr),
            yield_type_info.range,
        ));
    }

    // Mark as unsafe if it's a runtime Python file
    // and the body has more than one statement in it.
    let applicability = if is_stub || function_def.body.len() == 1 {
        Applicability::Safe
    } else {
        Applicability::Unsafe
    };
    Some(Fix::applicable_edits(edit, rest, applicability))
}

fn get_edit(expr: &ast::Expr) -> Option<Edit> {
    let ast::Expr::Attribute(ast::ExprAttribute {
        value, attr, range, ..
    }) = expr
    else {
        return None;
    };

    let new_return = match attr.as_str() {
        "Generator" => "Iterator",
        "AsyncGenerator" => "AsyncIterator",
        _ => return None,
    };

    let module = match value.as_ref() {
        ast::Expr::Name(ast::ExprName { id, .. }) => id.to_owned(),
        ast::Expr::Attribute(ast::ExprAttribute { attr, value, .. }) => match value.as_ref() {
            ast::Expr::Name(ast::ExprName { id, .. }) => format!("{id}.{attr}"),
            _ => return None,
        },
        _ => return None,
    };

    if !["typing", "typing_extensions", "collections.abc"].contains(&module.as_str()) {
        return None;
    }

    let repl = format!("{module}.{new_return}");
    Some(Edit::range_replacement(repl, range.to_owned()))
}
