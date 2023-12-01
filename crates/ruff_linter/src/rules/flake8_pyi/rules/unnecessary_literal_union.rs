use ast::{ExprSubscript, Operator};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

use crate::rules::flake8_pyi::helpers::traverse_union;

/// ## What it does
/// Checks for the presence of multiple literal types in a union.
///
/// ## Why is this bad?
/// Literal types accept multiple arguments and it is clearer to specify them
/// as a single literal.
///
/// ## Example
/// ```python
/// from typing import Literal
///
/// field: Literal[1] | Literal[2]
/// ```
///
/// Use instead:
/// ```python
/// from typing import Literal
///
/// field: Literal[1, 2]
/// ```
#[violation]
pub struct UnnecessaryLiteralUnion {
    members: Vec<String>,
}

impl Violation for UnnecessaryLiteralUnion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Multiple literal members in a union. Use a single literal, e.g. `Literal[{}]`",
            self.members.join(", ")
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with a single `Literal`",))
    }
}

fn concatenate_bin_ors(exprs: Vec<&Expr>) -> Expr {
    let mut exprs = exprs.into_iter();
    let first = exprs.next().unwrap();
    exprs.fold((*first).clone(), |acc, expr| {
        Expr::BinOp(ast::ExprBinOp {
            left: Box::new(acc),
            op: Operator::BitOr,
            right: Box::new((*expr).clone()),
            range: TextRange::default(),
        })
    })
}

fn make_union(subscript: &ExprSubscript, exprs: Vec<&Expr>) -> Expr {
    Expr::Subscript(ast::ExprSubscript {
        value: subscript.value.clone(),
        slice: Box::new(Expr::Tuple(ast::ExprTuple {
            elts: exprs.into_iter().map(|expr| (*expr).clone()).collect(),
            range: TextRange::default(),
            ctx: ast::ExprContext::Load,
        })),
        range: TextRange::default(),
        ctx: ast::ExprContext::Load,
    })
}

fn make_literal_expr(subscript: Option<Expr>, exprs: Vec<&Expr>) -> Expr {
    let use_subscript = if let subscript @ Some(_) = subscript {
        subscript.unwrap().clone()
    } else {
        Expr::Name(ast::ExprName {
            id: "Literal".to_string(),
            range: TextRange::default(),
            ctx: ast::ExprContext::Load,
        })
    };
    Expr::Subscript(ast::ExprSubscript {
        value: Box::new(use_subscript),
        slice: Box::new(Expr::Tuple(ast::ExprTuple {
            elts: exprs.into_iter().map(|expr| (*expr).clone()).collect(),
            range: TextRange::default(),
            ctx: ast::ExprContext::Load,
        })),
        range: TextRange::default(),
        ctx: ast::ExprContext::Load,
    })
}

/// PYI030
pub(crate) fn unnecessary_literal_union<'a>(checker: &mut Checker, expr: &'a Expr) {
    let mut literal_exprs = Vec::new();
    let mut other_exprs = Vec::new();

    // for the sake of consistency and correctness, we'll use the first `Literal` subscript attribute
    // to construct the fix
    let mut literal_subscript = None;
    let mut total_literals = 0;

    // Split members into `literal_exprs` if they are a `Literal` annotation  and `other_exprs` otherwise
    let mut collect_literal_expr = |expr: &'a Expr, _| {
        if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if checker.semantic().match_typing_expr(value, "Literal") {
                total_literals += 1;

                if literal_subscript.is_none() {
                    literal_subscript = Some(*value.clone());
                }

                // flatten already-unioned literals to later union again
                if let Expr::Tuple(ast::ExprTuple {
                    elts,
                    range: _,
                    ctx: _,
                }) = slice.as_ref()
                {
                    for expr in elts {
                        literal_exprs.push(expr);
                    }
                } else {
                    literal_exprs.push(slice.as_ref());
                }
            }
        } else {
            other_exprs.push(expr);
        }
    };

    // Traverse the union, collect all members, split out the literals from the rest.
    traverse_union(&mut collect_literal_expr, checker.semantic(), expr, None);

    let union_subscript = expr.as_subscript_expr();
    if union_subscript.is_some_and(|subscript| {
        !checker
            .semantic()
            .match_typing_expr(&subscript.value, "Union")
    }) {
        return;
    }

    // Raise a violation if more than one.
    if total_literals > 1 {
        let literal_members: Vec<String> = literal_exprs
            .clone()
            .into_iter()
            .map(|expr| checker.locator().slice(expr).to_string())
            .collect();

        let mut diagnostic = Diagnostic::new(
            UnnecessaryLiteralUnion {
                members: literal_members.clone(),
            },
            expr.range(),
        );

        if checker.settings.preview.is_enabled() {
            let literals =
                make_literal_expr(literal_subscript, literal_exprs.into_iter().collect());

            if other_exprs.is_empty() {
                // if the union is only literals, we just replace the whole thing with a single literal
                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    checker.generator().expr(&literals),
                    expr.range(),
                )));
            } else {
                let mut expr_vec: Vec<&Expr> = other_exprs.clone().into_iter().collect();
                expr_vec.insert(0, &literals);

                let content = if let Some(subscript) = union_subscript {
                    checker.generator().expr(&make_union(subscript, expr_vec))
                } else {
                    checker.generator().expr(&concatenate_bin_ors(expr_vec))
                };

                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    content,
                    expr.range(),
                )));
            }
        }

        checker.diagnostics.push(diagnostic);
    }
}
