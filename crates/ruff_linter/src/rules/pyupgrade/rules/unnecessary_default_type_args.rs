use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary default type arguments for `Generator` and
/// `AsyncGenerator` on Python 3.13+.
///
/// ## Why is this bad?
/// Python 3.13 introduced the ability for type parameters to specify default
/// values. Following this change, several standard-library classes were
/// updated to add default values for some of their type parameters. For
/// example, `Generator[int]` is now equivalent to
/// `Generator[int, None, None]`, as the second and third type parameters of
/// `Generator` now default to `None`.
///
/// Omitting type arguments that match the default values can make the code
/// more concise and easier to read.
///
/// ## Example
///
/// ```python
/// from collections.abc import Generator, AsyncGenerator
///
///
/// def sync_gen() -> Generator[int, None, None]:
///     yield 42
///
///
/// async def async_gen() -> AsyncGenerator[int, None]:
///     yield 42
/// ```
///
/// Use instead:
///
/// ```python
/// from collections.abc import Generator, AsyncGenerator
///
///
/// def sync_gen() -> Generator[int]:
///     yield 42
///
///
/// async def async_gen() -> AsyncGenerator[int]:
///     yield 42
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the type annotation contains comments.
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [PEP 696 – Type Defaults for Type Parameters](https://peps.python.org/pep-0696/)
/// - [Annotating generators and coroutines](https://docs.python.org/3/library/typing.html#annotating-generators-and-coroutines)
/// - [Python documentation: `typing.Generator`](https://docs.python.org/3/library/typing.html#typing.Generator)
/// - [Python documentation: `typing.AsyncGenerator`](https://docs.python.org/3/library/typing.html#typing.AsyncGenerator)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryDefaultTypeArgs;

impl AlwaysFixableViolation for UnnecessaryDefaultTypeArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary default type arguments".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove default type arguments".to_string()
    }
}

/// UP043
pub(crate) fn unnecessary_default_type_args(checker: &Checker, expr: &Expr) {
    let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr else {
        return;
    };

    let Expr::Tuple(ast::ExprTuple {
        elts,
        ctx: _,
        range: _,
        parenthesized: _,
    }) = slice.as_ref()
    else {
        return;
    };

    // The type annotation must be `Generator` or `AsyncGenerator`.
    let Some(type_annotation) = DefaultedTypeAnnotation::from_expr(value, checker.semantic())
    else {
        return;
    };

    let valid_elts = type_annotation.trim_unnecessary_defaults(elts);

    // If we didn't trim any elements, then the default type arguments are necessary.
    if *elts == valid_elts {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnnecessaryDefaultTypeArgs, expr.range());

    let applicability = if checker
        .comment_ranges()
        .has_comments(expr, checker.source())
    {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    diagnostic.set_fix(Fix::applicable_edit(
        Edit::range_replacement(
            checker
                .generator()
                .expr(&Expr::Subscript(ast::ExprSubscript {
                    value: value.clone(),
                    slice: Box::new(if let [elt] = valid_elts.as_slice() {
                        elt.clone()
                    } else {
                        Expr::Tuple(ast::ExprTuple {
                            elts: valid_elts,
                            ctx: ast::ExprContext::Load,
                            range: TextRange::default(),
                            parenthesized: true,
                        })
                    }),
                    ctx: ast::ExprContext::Load,
                    range: TextRange::default(),
                })),
            expr.range(),
        ),
        applicability,
    ));
    checker.report_diagnostic(diagnostic);
}

/// Trim trailing `None` literals from the given elements.
///
/// For example, given `[int, None, None]`, return `[int]`.
fn trim_trailing_none(elts: &[Expr]) -> &[Expr] {
    match elts.iter().rposition(|elt| !elt.is_none_literal_expr()) {
        Some(trimmed_last_index) => elts[..=trimmed_last_index].as_ref(),
        None => &[],
    }
}

/// Type annotations that include default type arguments as of Python 3.13.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefaultedTypeAnnotation {
    /// `typing.Generator[YieldType, SendType = None, ReturnType = None]`
    Generator,
    /// `typing.AsyncGenerator[YieldType, SendType = None]`
    AsyncGenerator,
}

impl DefaultedTypeAnnotation {
    /// Returns the [`DefaultedTypeAnnotation`], if the given expression is a type annotation that
    /// includes default type arguments.
    fn from_expr(expr: &Expr, semantic: &ruff_python_semantic::SemanticModel) -> Option<Self> {
        let qualified_name = semantic.resolve_qualified_name(expr)?;

        if semantic.match_typing_qualified_name(&qualified_name, "Generator")
            || matches!(
                qualified_name.segments(),
                ["collections", "abc", "Generator"]
            )
        {
            Some(Self::Generator)
        } else if semantic.match_typing_qualified_name(&qualified_name, "AsyncGenerator")
            || matches!(
                qualified_name.segments(),
                ["collections", "abc", "AsyncGenerator"]
            )
        {
            Some(Self::AsyncGenerator)
        } else {
            None
        }
    }

    /// Trim any unnecessary default type arguments from the given elements.
    fn trim_unnecessary_defaults(self, elts: &[Expr]) -> Vec<Expr> {
        match self {
            Self::Generator => {
                // Check only if the number of elements is 2 or 3 (e.g., `Generator[int, None]` or `Generator[int, None, None]`).
                // Otherwise, ignore (e.g., `Generator[]`, `Generator[int]`, `Generator[int, None, None, None]`)
                if elts.len() != 2 && elts.len() != 3 {
                    return elts.to_vec();
                }

                std::iter::once(elts[0].clone())
                    .chain(trim_trailing_none(&elts[1..]).iter().cloned())
                    .collect::<Vec<_>>()
            }
            Self::AsyncGenerator => {
                // Check only if the number of elements is 2 (e.g., `AsyncGenerator[int, None]`).
                // Otherwise, ignore (e.g., `AsyncGenerator[]`, `AsyncGenerator[int]`, `AsyncGenerator[int, None, None]`)
                if elts.len() != 2 {
                    return elts.to_vec();
                }

                std::iter::once(elts[0].clone())
                    .chain(trim_trailing_none(&elts[1..]).iter().cloned())
                    .collect::<Vec<_>>()
            }
        }
    }
}
