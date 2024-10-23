use ast::ExprContext;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::pep_604_union;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of multiple `type`s in a union.
///
/// ## Why is this bad?
/// `type[T | S]` has identical semantics to `type[T] | type[S]` in a type
/// annotation, but is cleaner and more concise.
///
/// ## Example
/// ```pyi
/// field: type[int] | type[float] | str
/// ```
///
/// Use instead:
/// ```pyi
/// field: type[int | float] | str
/// ```
#[violation]
pub struct UnnecessaryTypeUnion {
    members: Vec<Name>,
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
    let semantic = checker.semantic();

    // The `|` operator isn't always safe to allow to runtime-evaluated annotations.
    if semantic.execution_context().is_runtime() {
        return;
    }

    // Check if `union` is a PEP604 union (e.g. `float | int`) or a `typing.Union[float, int]`
    let subscript = union.as_subscript_expr();
    if subscript.is_some_and(|subscript| !semantic.match_typing_expr(&subscript.value, "Union")) {
        return;
    }

    let mut type_exprs: Vec<&Expr> = Vec::new();
    let mut other_exprs: Vec<&Expr> = Vec::new();

    let mut collect_type_exprs = |expr: &'a Expr, _parent: &'a Expr| match expr {
        Expr::Subscript(ast::ExprSubscript { slice, value, .. }) => {
            if semantic.match_builtin_expr(value, "type") {
                type_exprs.push(slice);
            } else {
                other_exprs.push(expr);
            }
        }
        _ => other_exprs.push(expr),
    };

    traverse_union(&mut collect_type_exprs, semantic, union);

    if type_exprs.len() > 1 {
        let type_members: Vec<Name> = type_exprs
            .clone()
            .into_iter()
            .map(|type_expr| Name::new(checker.locator().slice(type_expr)))
            .collect();

        let mut diagnostic = Diagnostic::new(
            UnnecessaryTypeUnion {
                members: type_members.clone(),
                is_pep604_union: subscript.is_none(),
            },
            union.range(),
        );

        if semantic.has_builtin_binding("type") {
            let content = if let Some(subscript) = subscript {
                let types = &Expr::Subscript(ast::ExprSubscript {
                    value: Box::new(Expr::Name(ast::ExprName {
                        id: Name::new_static("type"),
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
                            parenthesized: true,
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
                            parenthesized: true,
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
                        id: Name::new_static("type"),
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
