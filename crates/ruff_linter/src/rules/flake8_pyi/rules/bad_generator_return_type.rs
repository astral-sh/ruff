use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

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
#[derive(ViolationMetadata)]
pub(crate) struct GeneratorReturnFromIterMethod {
    return_type: Iterator,
    method: Method,
}

impl Violation for GeneratorReturnFromIterMethod {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let GeneratorReturnFromIterMethod {
            return_type,
            method,
        } = self;
        format!("Use `{return_type}` as the return value for simple `{method}` methods")
    }

    fn fix_title(&self) -> Option<String> {
        let GeneratorReturnFromIterMethod {
            return_type,
            method,
        } = self;
        Some(format!(
            "Convert the return annotation of your `{method}` method to `{return_type}`"
        ))
    }
}

/// PYI058
pub(crate) fn bad_generator_return_type(function_def: &ast::StmtFunctionDef, checker: &Checker) {
    if function_def.is_async {
        return;
    }

    let name = function_def.name.as_str();

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

    // Determine the module from which the existing annotation is imported (e.g., `typing` or
    // `collections.abc`)
    let (method, module, member) = {
        let Some(qualified_name) = semantic.resolve_qualified_name(map_subscript(returns)) else {
            return;
        };
        match (name, qualified_name.segments()) {
            ("__iter__", ["typing", "Generator"]) => {
                (Method::Iter, Module::Typing, Generator::Generator)
            }
            ("__aiter__", ["typing", "AsyncGenerator"]) => {
                (Method::AIter, Module::Typing, Generator::AsyncGenerator)
            }
            ("__iter__", ["typing_extensions", "Generator"]) => {
                (Method::Iter, Module::TypingExtensions, Generator::Generator)
            }
            ("__aiter__", ["typing_extensions", "AsyncGenerator"]) => (
                Method::AIter,
                Module::TypingExtensions,
                Generator::AsyncGenerator,
            ),
            ("__iter__", ["collections", "abc", "Generator"]) => {
                (Method::Iter, Module::CollectionsAbc, Generator::Generator)
            }
            ("__aiter__", ["collections", "abc", "AsyncGenerator"]) => (
                Method::AIter,
                Module::CollectionsAbc,
                Generator::AsyncGenerator,
            ),
            _ => return,
        }
    };

    // `Generator` allows three type parameters; `AsyncGenerator` allows two.
    // If type parameters are present,
    // check that all parameters except the first one are either `typing.Any` or `None`:
    // - if so, collect information on the first parameter for use in the rule's autofix;
    // - if not, don't emit the diagnostic
    let yield_type_info = match returns {
        ast::Expr::Subscript(ast::ExprSubscript { slice, .. }) => match slice.as_ref() {
            ast::Expr::Tuple(slice_tuple) => {
                if !slice_tuple
                    .iter()
                    .skip(1)
                    .all(|element| is_any_or_none(element, semantic))
                {
                    return;
                }
                let yield_type = match (name, slice_tuple.elts.as_slice()) {
                    ("__iter__", [yield_type, _, _]) => yield_type,
                    ("__aiter__", [yield_type, _]) => yield_type,
                    _ => return,
                };
                Some(YieldTypeInfo {
                    expr: yield_type,
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
    if !checker.source_type.is_stub() {
        let mut yield_encountered = false;
        for stmt in &function_def.body {
            match stmt {
                ast::Stmt::Pass(_) => continue,
                ast::Stmt::Return(ast::StmtReturn { value, .. }) => {
                    if let Some(ret_val) = value {
                        if yield_encountered && !ret_val.is_none_literal_expr() {
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
    }
    let mut diagnostic = Diagnostic::new(
        GeneratorReturnFromIterMethod {
            return_type: member.to_iter(),
            method,
        },
        function_def.identifier(),
    );

    diagnostic.try_set_fix(|| {
        generate_fix(
            function_def,
            returns,
            yield_type_info,
            module,
            member,
            checker,
        )
    });

    checker.report_diagnostic(diagnostic);
}

/// Returns `true` if the [`ast::Expr`] is a `None` literal or a `typing.Any` expression.
fn is_any_or_none(expr: &ast::Expr, semantic: &SemanticModel) -> bool {
    expr.is_none_literal_expr() || semantic.match_typing_expr(expr, "Any")
}

/// Generate a [`Fix`] to convert the return type annotation to `Iterator` or `AsyncIterator`.
fn generate_fix(
    function_def: &ast::StmtFunctionDef,
    returns: &ast::Expr,
    yield_type_info: Option<YieldTypeInfo>,
    module: Module,
    member: Generator,
    checker: &Checker,
) -> anyhow::Result<Fix> {
    let expr = map_subscript(returns);

    let (import_edit, binding) = checker.importer().get_or_import_symbol(
        &ImportRequest::import_from(&module.to_string(), &member.to_iter().to_string()),
        expr.start(),
        checker.semantic(),
    )?;
    let binding_edit = Edit::range_replacement(binding, expr.range());
    let yield_edit = yield_type_info.map(|yield_type_info| {
        Edit::range_replacement(
            checker.generator().expr(yield_type_info.expr),
            yield_type_info.range(),
        )
    });

    // Mark as unsafe if it's a runtime Python file and the body has more than one statement in it.
    let applicability = if checker.source_type.is_stub() || function_def.body.len() == 1 {
        Applicability::Safe
    } else {
        Applicability::Unsafe
    };

    Ok(Fix::applicable_edits(
        import_edit,
        std::iter::once(binding_edit).chain(yield_edit),
        applicability,
    ))
}

#[derive(Debug)]
struct YieldTypeInfo<'a> {
    expr: &'a ast::Expr,
    range: TextRange,
}

impl Ranged for YieldTypeInfo<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Module {
    Typing,
    TypingExtensions,
    CollectionsAbc,
}

impl std::fmt::Display for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Module::Typing => write!(f, "typing"),
            Module::TypingExtensions => write!(f, "typing_extensions"),
            Module::CollectionsAbc => write!(f, "collections.abc"),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Method {
    Iter,
    AIter,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Method::Iter => write!(f, "__iter__"),
            Method::AIter => write!(f, "__aiter__"),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Generator {
    Generator,
    AsyncGenerator,
}

impl std::fmt::Display for Generator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Generator::Generator => write!(f, "Generator"),
            Generator::AsyncGenerator => write!(f, "AsyncGenerator"),
        }
    }
}

impl Generator {
    fn to_iter(self) -> Iterator {
        match self {
            Generator::Generator => Iterator::Iterator,
            Generator::AsyncGenerator => Iterator::AsyncIterator,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Iterator {
    Iterator,
    AsyncIterator,
}

impl std::fmt::Display for Iterator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Iterator::Iterator => write!(f, "Iterator"),
            Iterator::AsyncIterator => write!(f, "AsyncIterator"),
        }
    }
}
