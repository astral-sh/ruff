use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, ExprBinOp, Operator};
use ruff_python_semantic::{analyze::typing::traverse_union, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `typing.NoReturn` and `typing.Never` in union types.
///
/// ## Why is this bad?
/// `typing.NoReturn` and `typing.Never` are special types, used to indicate
/// that a function never returns, or that a type has no values.
///
/// Including `typing.NoReturn` or `typing.Never` in a union type is redundant,
/// as, e.g., `typing.Never | T` is equivalent to `T`.
///
/// ## Example
///
/// ```python
/// from typing import Never
///
///
/// def func() -> Never | int: ...
/// ```
///
/// Use instead:
///
/// ```python
/// def func() -> int: ...
/// ```
///
/// ## References
/// - [Python documentation: `typing.Never`](https://docs.python.org/3/library/typing.html#typing.Never)
/// - [Python documentation: `typing.NoReturn`](https://docs.python.org/3/library/typing.html#typing.NoReturn)
#[derive(ViolationMetadata)]
pub(crate) struct NeverUnion {
    never_like: NeverLike,
    union_like: UnionLike,
}

impl Violation for NeverUnion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            never_like,
            union_like,
        } = self;
        match union_like {
            UnionLike::PEP604 => {
                format!("`{never_like} | T` is equivalent to `T`")
            }
            UnionLike::TypingUnion => {
                format!("`Union[{never_like}, T]` is equivalent to `T`")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Self { never_like, .. } = self;
        Some(format!("Remove `{never_like}`"))
    }
}

/// RUF020
pub(crate) fn never_union(checker: &Checker, expr: &Expr) {
    match expr {
        // Ex) `typing.NoReturn | int`
        Expr::BinOp(ast::ExprBinOp {
            op: Operator::BitOr,
            left,
            right,
            range: _,
        }) => {
            // Analyze the left-hand side of the `|` operator.
            if let Some(never_like) = NeverLike::from_expr(left, checker.semantic()) {
                let mut diagnostic = Diagnostic::new(
                    NeverUnion {
                        never_like,
                        union_like: UnionLike::PEP604,
                    },
                    left.range(),
                );
                // Avoid producing code that would raise an exception when
                // `Never | None` would be fixed to `None | None`.
                // Instead do not provide a fix. No action needed for `typing.Union`,
                // as `Union[None, None]` is valid Python.
                // See https://github.com/astral-sh/ruff/issues/14567.
                if !is_pep604_union_with_bare_none(checker.semantic()) {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        checker.locator().slice(right.as_ref()).to_string(),
                        expr.range(),
                    )));
                }
                checker.report_diagnostic(diagnostic);
            }

            // Analyze the right-hand side of the `|` operator.
            if let Some(never_like) = NeverLike::from_expr(right, checker.semantic()) {
                let mut diagnostic = Diagnostic::new(
                    NeverUnion {
                        never_like,
                        union_like: UnionLike::PEP604,
                    },
                    right.range(),
                );
                if !is_pep604_union_with_bare_none(checker.semantic()) {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        checker.locator().slice(left.as_ref()).to_string(),
                        expr.range(),
                    )));
                }
                checker.report_diagnostic(diagnostic);
            }
        }

        // Ex) `typing.Union[typing.NoReturn, int]`
        Expr::Subscript(ast::ExprSubscript {
            value,
            slice,
            ctx: _,
            range: _,
        }) if checker.semantic().match_typing_expr(value, "Union") => {
            let Expr::Tuple(tuple_slice) = &**slice else {
                return;
            };

            // Analyze each element of the `Union`.
            for elt in tuple_slice {
                if let Some(never_like) = NeverLike::from_expr(elt, checker.semantic()) {
                    // Collect the other elements of the `Union`.
                    let rest: Vec<Expr> = tuple_slice
                        .iter()
                        .filter(|other| *other != elt)
                        .cloned()
                        .collect();

                    // Ignore, e.g., `typing.Union[typing.NoReturn]`.
                    if rest.is_empty() {
                        return;
                    }

                    let mut diagnostic = Diagnostic::new(
                        NeverUnion {
                            never_like,
                            union_like: UnionLike::TypingUnion,
                        },
                        elt.range(),
                    );
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        if let [only] = rest.as_slice() {
                            // Ex) `typing.Union[typing.NoReturn, int]` -> `int`
                            checker.locator().slice(only).to_string()
                        } else {
                            // Ex) `typing.Union[typing.NoReturn, int, str]` -> `typing.Union[int, str]`
                            checker
                                .generator()
                                .expr(&Expr::Subscript(ast::ExprSubscript {
                                    value: value.clone(),
                                    slice: Box::new(Expr::Tuple(ast::ExprTuple {
                                        elts: rest,
                                        ctx: ast::ExprContext::Load,
                                        range: TextRange::default(),
                                        parenthesized: true,
                                    })),
                                    ctx: ast::ExprContext::Load,
                                    range: TextRange::default(),
                                }))
                        },
                        expr.range(),
                    )));
                    checker.report_diagnostic(diagnostic);
                }
            }
        }

        _ => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnionLike {
    /// E.g., `typing.Union[int, str]`
    TypingUnion,
    /// E.g., `int | str`
    PEP604,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NeverLike {
    /// E.g., `typing.NoReturn`
    NoReturn,
    /// E.g., `typing.Never`
    Never,
}

impl NeverLike {
    fn from_expr(expr: &Expr, semantic: &ruff_python_semantic::SemanticModel) -> Option<Self> {
        let qualified_name = semantic.resolve_qualified_name(expr)?;
        if semantic.match_typing_qualified_name(&qualified_name, "NoReturn") {
            Some(NeverLike::NoReturn)
        } else if semantic.match_typing_qualified_name(&qualified_name, "Never") {
            Some(NeverLike::Never)
        } else {
            None
        }
    }
}

impl std::fmt::Display for NeverLike {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NeverLike::NoReturn => f.write_str("NoReturn"),
            NeverLike::Never => f.write_str("Never"),
        }
    }
}

/// Return `true` if this union is a [PEP 604 union] that contains `None`,
/// e.g. `int | Never | None`.
///
/// Autofixing these unions can be dangerous,
/// as `None | None` results in a runtime exception in Python.
///
/// [PEP 604 union]: https://docs.python.org/3/library/stdtypes.html#types-union
fn is_pep604_union_with_bare_none(semantic: &SemanticModel) -> bool {
    let enclosing_pep604_union = semantic
        .current_expressions()
        .skip(1)
        .take_while(|expr| {
            matches!(
                expr,
                Expr::BinOp(ExprBinOp {
                    op: Operator::BitOr,
                    ..
                })
            )
        })
        .last();

    let Some(enclosing_pep604_union) = enclosing_pep604_union else {
        return false;
    };

    let mut union_contains_bare_none = false;

    traverse_union(
        &mut |expr, _| {
            if matches!(expr, Expr::NoneLiteral(_)) {
                union_contains_bare_none = true;
            }
        },
        semantic,
        enclosing_pep604_union,
    );

    union_contains_bare_none
}
