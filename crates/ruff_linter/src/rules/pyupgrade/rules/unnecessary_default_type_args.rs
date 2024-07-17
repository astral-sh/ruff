use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary default type arguments.
///
/// ## Why is this bad?
/// Python 3.13 introduced a new feature: type defaults for type parameters.
/// Now, you can omit default type arguments for some types in the standard library (e.g., Generator, AsyncGenerator).
/// Including unnecessary default type arguments can make the code more verbose and less readable.
///
/// ## Examples
/// ```python
/// from typing import Generator, AsyncGenerator
///
/// def sync_gen() -> Generator[int, None, None]:
///     yield 42
///
/// async def async_gen() -> AsyncGenerator[int, None]:
///     yield 42
/// ```
///
/// Use instead:
/// ```python
/// from typing import Generator, AsyncGenerator
///
/// def sync_gen() -> Generator[int]:
///     yield 42
///
/// async def async_gen() -> AsyncGenerator[int]:
///     yield 42
/// ```
///
/// ## References
/// - [PEP 696 – Type Defaults for Type Parameters](https://peps.python.org/pep-0696/)
/// - [typing.Generator](https://docs.python.org/3.13/library/typing.html#typing.Generator)
/// - [typing.AsyncGenerator](https://docs.python.org/3.13/library/typing.html#typing.AsyncGenerator)
#[violation]
pub struct UnnecessaryDefaultTypeArgs;

impl AlwaysFixableViolation for UnnecessaryDefaultTypeArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary default type arguments")
    }

    fn fix_title(&self) -> String {
        format!("Remove unnecessary default type arguments")
    }
}

// UP043
pub(crate) fn unnecessary_default_type_args(checker: &mut Checker, expr: &Expr) {
    if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
        // Check if the type annotation is Generator or AsyncGenerator.
        let Some(type_annotation) = DefaultedTypeAnnotation::from_expr(value, checker.semantic())
        else {
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

        let valid_elts = type_annotation.trim_unnecessary_defaults(elts[..].as_ref());

        if *elts == valid_elts {
            return;
        }

        let mut diagnostic = Diagnostic::new(UnnecessaryDefaultTypeArgs, expr.range());

        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            checker
                .generator()
                .expr(&Expr::Subscript(ast::ExprSubscript {
                    value: value.clone(),
                    slice: Box::new(if valid_elts.len() == 1 {
                        valid_elts[0].clone()
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
        )));
        checker.diagnostics.push(diagnostic);
    }
}

fn trim_trailing_none(elts: &[Expr]) -> &[Expr] {
    // Trim trailing None literals.
    // e.g. [int, None, None] -> [int]
    match elts.iter().rposition(|elt| !elt.is_none_literal_expr()) {
        Some(trimmed_last_index) => elts[..=trimmed_last_index].as_ref(),
        None => &[],
    }
}

// Type annotations affected by the new feature(type defaults).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefaultedTypeAnnotation {
    Generator,      // typing.Generator[YieldType, SendType = None, ReturnType = None]
    AsyncGenerator, // typing.AsyncGenerator[YieldType, SendType = None]
}

impl DefaultedTypeAnnotation {
    fn from_expr(expr: &Expr, semantic: &ruff_python_semantic::SemanticModel) -> Option<Self> {
        let qualified_name = semantic.resolve_qualified_name(expr)?;
        if semantic.match_typing_qualified_name(&qualified_name, "Generator") {
            Some(DefaultedTypeAnnotation::Generator)
        } else if semantic.match_typing_qualified_name(&qualified_name, "AsyncGenerator") {
            Some(DefaultedTypeAnnotation::AsyncGenerator)
        } else {
            None
        }
    }

    fn trim_unnecessary_defaults(self, elts: &[Expr]) -> Vec<Expr> {
        match self {
            DefaultedTypeAnnotation::Generator => {
                // Check only if the number of elements is 2 or 3 (e.g. Generator[int, None] or Generator[int, None, None]).
                // Ignore otherwise (e.g. Generator[], Generator[int], Generator[int, None, None, None])
                if !(2 <= elts.len() && elts.len() <= 3) {
                    return elts.to_vec();
                }

                std::iter::once(elts[0].clone())
                    .chain(trim_trailing_none(&elts[1..]).iter().cloned())
                    .collect::<Vec<_>>()
            }
            DefaultedTypeAnnotation::AsyncGenerator => {
                // Check only if the number of elements is 2 (e.g. AsyncGenerator[int, None]).
                // Ignore otherwise (e.g. AsyncGenerator[], AsyncGenerator[int], AsyncGenerator[int, None, None])
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
