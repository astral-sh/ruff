use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    self as ast, Expr, ExprBinOp, ExprContext, ExprNoneLiteral, ExprSubscript, Operator,
};
use ruff_python_semantic::{
    analyze::typing::{traverse_literal, traverse_union},
    SemanticModel,
};
use ruff_text_size::{Ranged, TextRange};

use smallvec::SmallVec;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for redundant `Literal[None]` annotations.
///
/// ## Why is this bad?
/// While `Literal[None]` is a valid type annotation, it is semantically equivalent to `None`.
/// Prefer `None` over `Literal[None]` for both consistency and readability.
///
/// ## Example
/// ```python
/// from typing import Literal
///
/// Literal[None]
/// Literal[1, 2, 3, "foo", 5, None]
/// ```
///
/// Use instead:
/// ```python
/// from typing import Literal
///
/// None
/// Literal[1, 2, 3, "foo", 5] | None
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe unless the literal contains comments.
///
/// ## References
/// - [Typing documentation: Legal parameters for `Literal` at type check time](https://typing.readthedocs.io/en/latest/spec/literal.html#legal-parameters-for-literal-at-type-check-time)
#[derive(ViolationMetadata)]
pub(crate) struct RedundantNoneLiteral {
    other_literal_elements_seen: bool,
}

impl Violation for RedundantNoneLiteral {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if self.other_literal_elements_seen {
            "`Literal[None, ...]` can be replaced with `Literal[...] | None`".to_string()
        } else {
            "`Literal[None]` can be replaced with `None`".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some(if self.other_literal_elements_seen {
            "Replace with `Literal[...] | None`".to_string()
        } else {
            "Replace with `None`".to_string()
        })
    }
}

/// RUF037
pub(crate) fn redundant_none_literal<'a>(checker: &mut Checker, literal_expr: &'a Expr) {
    if !checker.semantic().seen_typing() {
        return;
    }

    let mut none_exprs: SmallVec<[&ExprNoneLiteral; 1]> = SmallVec::new();
    let mut other_literal_elements_seen = false;
    let mut literal_elements: Vec<&Expr> = Vec::new();
    let mut literal_subscript = None;
    if let Expr::Subscript(ast::ExprSubscript { value, .. }) = literal_expr {
        if checker.semantic().match_typing_expr(value, "Literal") {
            literal_subscript = Some(value.as_ref());
        }
    };

    let mut find_literal_elements = |expr: &'a Expr, _parent: &'a Expr| {
        if let Expr::NoneLiteral(none_expr) = expr {
            none_exprs.push(none_expr);
        } else {
            other_literal_elements_seen = true;
            literal_elements.push(expr);
        }
    };

    let Some(literal_subscript) = literal_subscript else {
        return;
    };

    traverse_literal(&mut find_literal_elements, checker.semantic(), literal_expr);
    if none_exprs.is_empty() {
        return;
    }

    // Provide a [`Fix`] when the complete `Literal` can be replaced. Applying the fix
    // can leave an unused import to be fixed by the `unused-import` rule.
    let fix = if other_literal_elements_seen {
        create_fix_edit_2(checker, literal_expr, literal_elements, literal_subscript).map(|edit| {
            Fix::applicable_edit(
                edit,
                if checker.comment_ranges().intersects(literal_expr.range()) {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                },
            )
        })
    } else {
        create_fix_edit(checker.semantic(), literal_expr).map(|edit| {
            Fix::applicable_edit(
                edit,
                if checker.comment_ranges().intersects(literal_expr.range()) {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                },
            )
        })
    };

    for none_expr in none_exprs {
        let mut diagnostic = Diagnostic::new(
            RedundantNoneLiteral {
                other_literal_elements_seen,
            },
            none_expr.range(),
        );
        if let Some(ref fix) = fix {
            diagnostic.set_fix(fix.clone());
        }
        checker.diagnostics.push(diagnostic);
    }
}

/// If possible, return a [`Fix`] for a violation of this rule.
///
/// Avoid producing code that would raise an exception when
/// `Literal[None] | None` would be fixed to `None | None`.
/// Instead, do not provide a fix. We don't need to worry about unions
/// that use [`typing.Union`], as `Union[None, None]` is valid Python.
/// See <https://github.com/astral-sh/ruff/issues/14567>.
///
/// [`typing.Union`]: https://docs.python.org/3/library/typing.html#typing.Union
fn create_fix_edit(semantic: &SemanticModel, literal_expr: &Expr) -> Option<Edit> {
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

    let mut is_fixable = true;
    if let Some(enclosing_pep604_union) = enclosing_pep604_union {
        traverse_union(
            &mut |expr, _| {
                if matches!(expr, Expr::NoneLiteral(_)) {
                    is_fixable = false;
                }
                if expr != literal_expr {
                    if let Expr::Subscript(ExprSubscript { value, slice, .. }) = expr {
                        if semantic.match_typing_expr(value, "Literal")
                            && matches!(**slice, Expr::NoneLiteral(_))
                        {
                            is_fixable = false;
                        }
                    }
                }
            },
            semantic,
            enclosing_pep604_union,
        );
    }

    is_fixable.then(|| Edit::range_replacement("None".to_string(), literal_expr.range()))
}

fn create_fix_edit_2(
    checker: &mut Checker,
    literal_expr: &Expr,
    literal_elements: Vec<&Expr>,
    literal_subscript: &Expr,
) -> Option<Edit> {
    let enclosing_pep604_union = checker
        .semantic()
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

    let mut is_fixable = true;
    if let Some(enclosing_pep604_union) = enclosing_pep604_union {
        traverse_union(
            &mut |expr, _| {
                if matches!(expr, Expr::NoneLiteral(_)) {
                    is_fixable = false;
                }
                if expr != literal_expr {
                    if let Expr::Subscript(ExprSubscript { value, slice, .. }) = expr {
                        if checker.semantic().match_typing_expr(value, "Literal")
                            && matches!(**slice, Expr::NoneLiteral(_))
                        {
                            is_fixable = false;
                        }
                    }
                }
            },
            checker.semantic(),
            enclosing_pep604_union,
        );
    }

    let bin_or = Expr::BinOp(ExprBinOp {
        range: TextRange::default(),
        left: Box::new(Expr::Subscript(ast::ExprSubscript {
            value: Box::new(literal_subscript.clone()),
            range: TextRange::default(),
            ctx: ExprContext::Load,
            slice: Box::new(Expr::Tuple(ast::ExprTuple {
                elts: literal_elements.into_iter().cloned().collect(),
                range: TextRange::default(),
                ctx: ExprContext::Load,
                parenthesized: true,
            })),
        })),
        op: ruff_python_ast::Operator::BitOr,
        right: Box::new(Expr::NoneLiteral(ExprNoneLiteral {
            range: TextRange::default(),
        })),
    });

    let content = checker.generator().expr(&bin_or);

    is_fixable.then(|| Edit::range_replacement(content, literal_expr.range()))
}
