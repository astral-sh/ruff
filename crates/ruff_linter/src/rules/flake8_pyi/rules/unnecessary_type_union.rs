use ast::{ExprContext, Operator};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextRange};

use crate::{checkers::ast::Checker, rules::flake8_pyi::helpers::traverse_union};

/// ## What it does
/// Checks for the presence of multiple `type`s in a union.
///
/// ## Why is this bad?
/// The `type` built-in function accepts unions, and it is clearer to
/// explicitly specify them as a single `type`.
///
/// ## Example
/// ```python
/// field: type[int] | type[float]
/// ```
///
/// Use instead:
/// ```python
/// field: type[int | float]
/// ```
#[violation]
pub struct UnnecessaryTypeUnion {
    members: Vec<String>,
    is_pep604_union: bool,
}

impl Violation for UnnecessaryTypeUnion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let union_str = if self.is_pep604_union {
            format!("{}", self.members.join(" | "))
        } else {
            format!("Union[{}]", self.members.join(", "))
        };

        format!(
            "Multiple `type` members in a union. Combine them into one, e.g., `type[{union_str}]`."
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Combine multiple `type` members"))
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

/// PYI055
pub(crate) fn unnecessary_type_union<'a>(checker: &mut Checker, union: &'a Expr) {
    // The `|` operator isn't always safe to allow to runtime-evaluated annotations.
    if checker.semantic().execution_context().is_runtime() {
        return;
    }

    // Check if `union` is a PEP604 union (e.g. `float | int`) or a `typing.Union[float, int]`
    let subscript = union.as_subscript_expr();
    if subscript.is_some_and(|subscript| {
        !checker
            .semantic()
            .match_typing_expr(&subscript.value, "Union")
    }) {
        return;
    }

    let mut type_exprs = Vec::new();

    let mut collect_type_exprs = |expr: &'a Expr, _| {
        let Some(subscript) = expr.as_subscript_expr() else {
            return;
        };
        if checker
            .semantic()
            .resolve_call_path(subscript.value.as_ref())
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["" | "builtins", "type"]))
        {
            type_exprs.push(&subscript.slice);
        }
    };

    traverse_union(&mut collect_type_exprs, checker.semantic(), union, None);

    if type_exprs.len() > 1 {
        let type_members: Vec<String> = type_exprs
            .clone()
            .into_iter()
            .map(|type_expr| checker.locator().slice(type_expr.as_ref()).to_string())
            .collect();

        let mut diagnostic = Diagnostic::new(
            UnnecessaryTypeUnion {
                members: type_members.clone(),
                is_pep604_union: subscript.is_none(),
            },
            union.range(),
        );

        if checker.semantic().is_builtin("type") {
            let content = if let Some(subscript) = subscript {
                checker
                    .generator()
                    .expr(&Expr::Subscript(ast::ExprSubscript {
                        value: Box::new(Expr::Name(ast::ExprName {
                            id: "type".into(),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                        })),
                        slice: Box::new(Expr::Subscript(ast::ExprSubscript {
                            value: subscript.value.clone(),
                            slice: Box::new(Expr::Tuple(ast::ExprTuple {
                                elts: type_members
                                    .into_iter()
                                    .map(|type_member| {
                                        Expr::Name(ast::ExprName {
                                            id: type_member,
                                            ctx: ExprContext::Load,
                                            range: TextRange::default(),
                                        })
                                    })
                                    .collect(),
                                ctx: ExprContext::Load,
                                range: TextRange::default(),
                            })),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                        })),
                        ctx: ExprContext::Load,
                        range: TextRange::default(),
                    }))
            } else {
                checker
                    .generator()
                    .expr(&Expr::Subscript(ast::ExprSubscript {
                        value: Box::new(Expr::Name(ast::ExprName {
                            id: "type".into(),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                        })),
                        slice: Box::new(concatenate_bin_ors(
                            type_exprs
                                .clone()
                                .into_iter()
                                .map(std::convert::AsRef::as_ref)
                                .collect(),
                        )),
                        ctx: ExprContext::Load,
                        range: TextRange::default(),
                    }))
            };

            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                content,
                union.range(),
            )));
        }

        checker.diagnostics.push(diagnostic);
    }
}
