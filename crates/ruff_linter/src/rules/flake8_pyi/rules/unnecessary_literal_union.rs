use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::pep_604_union;
use ruff_python_ast::{self as ast, Expr, ExprContext};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

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

/// PYI030
pub(crate) fn unnecessary_literal_union<'a>(checker: &mut Checker, expr: &'a Expr) {
    let mut literal_exprs = Vec::new();
    let mut other_exprs = Vec::new();

    // For the sake of consistency, use the first `Literal` subscript to construct the fix.
    let mut literal_subscript = None;
    let mut total_literals = 0;

    // Split members into `literal_exprs` if they are a `Literal` annotation  and `other_exprs` otherwise
    let mut collect_literal_expr = |expr: &'a Expr, _parent: &'a Expr| {
        if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if checker.semantic().match_typing_expr(value, "Literal") {
                total_literals += 1;

                if literal_subscript.is_none() {
                    literal_subscript = Some(value.as_ref());
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
    traverse_union(&mut collect_literal_expr, checker.semantic(), expr);

    let union_subscript = expr.as_subscript_expr();
    if union_subscript.is_some_and(|subscript| {
        !checker
            .semantic()
            .match_typing_expr(&subscript.value, "Union")
    }) {
        return;
    }

    // If there are no literal members, we don't need to do anything.
    let Some(literal_subscript) = literal_subscript else {
        return;
    };
    if total_literals == 0 || total_literals == 1 {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralUnion {
            members: literal_exprs
                .iter()
                .map(|expr| checker.locator().slice(expr).to_string())
                .collect(),
        },
        expr.range(),
    );

    diagnostic.set_fix({
        let literal = Expr::Subscript(ast::ExprSubscript {
            value: Box::new(literal_subscript.clone()),
            slice: Box::new(Expr::Tuple(ast::ExprTuple {
                elts: literal_exprs.into_iter().cloned().collect(),
                range: TextRange::default(),
                ctx: ExprContext::Load,
            })),
            range: TextRange::default(),
            ctx: ExprContext::Load,
        });

        if other_exprs.is_empty() {
            // if the union is only literals, we just replace the whole thing with a single literal
            Fix::safe_edit(Edit::range_replacement(
                checker.generator().expr(&literal),
                expr.range(),
            ))
        } else {
            let elts: Vec<Expr> = std::iter::once(literal)
                .chain(other_exprs.into_iter().cloned())
                .collect();

            let content = if let Some(union) = union_subscript {
                checker
                    .generator()
                    .expr(&Expr::Subscript(ast::ExprSubscript {
                        value: union.value.clone(),
                        slice: Box::new(Expr::Tuple(ast::ExprTuple {
                            elts,
                            range: TextRange::default(),
                            ctx: ExprContext::Load,
                        })),
                        range: TextRange::default(),
                        ctx: ExprContext::Load,
                    }))
            } else {
                checker.generator().expr(&pep_604_union(&elts))
            };

            Fix::safe_edit(Edit::range_replacement(content, expr.range()))
        }
    });

    checker.diagnostics.push(diagnostic);
}
