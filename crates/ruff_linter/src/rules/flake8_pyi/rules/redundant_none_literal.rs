use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    self as ast, Expr, ExprBinOp, ExprContext, ExprNoneLiteral, ExprSubscript, Operator,
};
use ruff_python_semantic::analyze::typing::{traverse_literal, traverse_union};
use ruff_text_size::{Ranged, TextRange};

use smallvec::SmallVec;

use crate::{checkers::ast::Checker, settings::types::PythonVersion};

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
/// ## Fix safety and availability
/// This rule's fix is marked as safe unless the literal contains comments.
///
/// There is currently no fix available if there are other elements in the `Literal` slice aside
/// from `None` and [`target-version`] is set to Python 3.9 or lower, as the fix always uses the
/// `|` syntax to create unions rather than `typing.Union`, and the `|` syntax for unions was added
/// in Python 3.10.
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

/// PYI061
pub(crate) fn redundant_none_literal<'a>(checker: &mut Checker, literal_expr: &'a Expr) {
    let semantic = checker.semantic();

    if !semantic.seen_typing() {
        return;
    }

    let Expr::Subscript(ast::ExprSubscript {
        value: literal_subscript,
        ..
    }) = literal_expr
    else {
        return;
    };

    let mut none_exprs: SmallVec<[&ExprNoneLiteral; 1]> = SmallVec::new();
    let mut literal_elements = vec![];

    let mut partition_literal_elements = |expr: &'a Expr, _parent: &'a Expr| {
        if let Expr::NoneLiteral(none_expr) = expr {
            none_exprs.push(none_expr);
        } else {
            literal_elements.push(expr);
        }
    };

    traverse_literal(&mut partition_literal_elements, semantic, literal_expr);

    if none_exprs.is_empty() {
        return;
    }

    let other_literal_elements_seen = !literal_elements.is_empty();

    // N.B. Applying the fix can leave an unused import to be fixed by the `unused-import` rule.
    let fix =
        create_fix_edit(checker, literal_expr, literal_subscript, literal_elements).map(|edit| {
            Fix::applicable_edit(
                edit,
                if checker.comment_ranges().intersects(literal_expr.range()) {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                },
            )
        });

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
fn create_fix_edit(
    checker: &Checker,
    literal_expr: &Expr,
    literal_subscript: &Expr,
    literal_elements: Vec<&Expr>,
) -> Option<Edit> {
    let semantic = checker.semantic();

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

    if let Some(enclosing_pep604_union) = enclosing_pep604_union {
        let mut is_fixable = true;

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

        if !is_fixable {
            return None;
        }
    }

    if literal_elements.is_empty() {
        return Some(Edit::range_replacement(
            "None".to_string(),
            literal_expr.range(),
        ));
    }

    if checker.settings.target_version < PythonVersion::Py310 {
        return None;
    }

    let bin_or = Expr::BinOp(ExprBinOp {
        range: TextRange::default(),
        left: Box::new(Expr::Subscript(ast::ExprSubscript {
            value: Box::new(literal_subscript.clone()),
            range: TextRange::default(),
            ctx: ExprContext::Load,
            slice: Box::new(if literal_elements.len() > 1 {
                Expr::Tuple(ast::ExprTuple {
                    elts: literal_elements.into_iter().cloned().collect(),
                    range: TextRange::default(),
                    ctx: ExprContext::Load,
                    parenthesized: true,
                })
            } else {
                literal_elements[0].clone()
            }),
        })),
        op: ruff_python_ast::Operator::BitOr,
        right: Box::new(Expr::NoneLiteral(ExprNoneLiteral {
            range: TextRange::default(),
        })),
    });

    let content = checker.generator().expr(&bin_or);
    Some(Edit::range_replacement(content, literal_expr.range()))
}
