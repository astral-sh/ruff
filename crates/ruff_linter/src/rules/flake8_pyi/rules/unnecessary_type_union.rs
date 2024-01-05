use ast::ExprContext;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::pep_604_union;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

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
    let mut other_exprs = Vec::new();

    let mut collect_type_exprs = |expr: &'a Expr, _parent: &'a Expr| {
        let subscript = expr.as_subscript_expr();

        if subscript.is_none() {
            other_exprs.push(expr);
        } else {
            let unwrapped = subscript.unwrap();
            if checker
                .semantic()
                .resolve_call_path(unwrapped.value.as_ref())
                .is_some_and(|call_path| matches!(call_path.as_slice(), ["" | "builtins", "type"]))
            {
                type_exprs.push(unwrapped.slice.as_ref());
            } else {
                other_exprs.push(expr);
            }
        }
    };

    traverse_union(&mut collect_type_exprs, checker.semantic(), union);

    if type_exprs.len() > 1 {
        let type_members: Vec<String> = type_exprs
            .clone()
            .into_iter()
            .map(|type_expr| checker.locator().slice(type_expr).to_string())
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
                let types = &Expr::Subscript(ast::ExprSubscript {
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
                });

                if other_exprs.is_empty() {
                    checker.generator().expr(types)
                } else {
                    let mut exprs = Vec::new();
                    exprs.push(types);
                    exprs.extend(other_exprs);

                    let union = Expr::Subscript(ast::ExprSubscript {
                        value: subscript.value.clone(),
                        slice: Box::new(Expr::Tuple(ast::ExprTuple {
                            elts: exprs.into_iter().cloned().collect(),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                        })),
                        ctx: ExprContext::Load,
                        range: TextRange::default(),
                    });

                    checker.generator().expr(&union)
                }
            } else {
                let elts: Vec<Expr> = type_exprs.into_iter().cloned().collect();
                let types = Expr::Subscript(ast::ExprSubscript {
                    value: Box::new(Expr::Name(ast::ExprName {
                        id: "type".into(),
                        ctx: ExprContext::Load,
                        range: TextRange::default(),
                    })),
                    slice: Box::new(pep_604_union(&elts)),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                });

                if other_exprs.is_empty() {
                    checker.generator().expr(&types)
                } else {
                    let elts: Vec<Expr> = std::iter::once(types)
                        .chain(other_exprs.into_iter().cloned())
                        .collect();
                    checker.generator().expr(&pep_604_union(&elts))
                }
            };

            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                content,
                union.range(),
            )));
        }

        checker.diagnostics.push(diagnostic);
    }
}
