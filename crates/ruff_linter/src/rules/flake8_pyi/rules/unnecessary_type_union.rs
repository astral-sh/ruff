use ast::ExprContext;
use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the type annotation contains comments.
///
/// Note that while the fix may flatten nested unions into a single top-level union,
/// the semantics of the annotation will remain unchanged.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryTypeUnion {
    members: Vec<Name>,
    union_kind: UnionKind,
}

impl Violation for UnnecessaryTypeUnion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let union_str = match self.union_kind {
            UnionKind::PEP604 => self.members.join(" | "),
            UnionKind::TypingUnion => format!("Union[{}]", self.members.join(", ")),
        };

        format!(
            "Multiple `type` members in a union. Combine them into one, e.g., `type[{union_str}]`."
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Combine multiple `type` members".to_string())
    }
}

/// PYI055
pub(crate) fn unnecessary_type_union<'a>(checker: &Checker, union: &'a Expr) {
    let semantic = checker.semantic();

    // The `|` operator isn't always safe to allow to runtime-evaluated annotations.
    if semantic.execution_context().is_runtime() {
        return;
    }

    // Check if `union` is a PEP604 union (e.g. `float | int`) or a `typing.Union[float, int]`
    let subscript = union.as_subscript_expr();
    let mut union_kind = match subscript {
        Some(subscript) => {
            if !semantic.match_typing_expr(&subscript.value, "Union") {
                return;
            }
            UnionKind::TypingUnion
        }
        None => UnionKind::PEP604,
    };

    let mut type_exprs: Vec<&Expr> = Vec::new();
    let mut other_exprs: Vec<&Expr> = Vec::new();

    let mut collect_type_exprs = |expr: &'a Expr, parent: &'a Expr| {
        // If a PEP604-style union is used within a `typing.Union`, then the fix can
        // use PEP604-style unions.
        if matches!(parent, Expr::BinOp(_)) {
            union_kind = UnionKind::PEP604;
        }
        match expr {
            Expr::Subscript(ast::ExprSubscript { slice, value, .. }) => {
                // The annotation `type[a, b]` is not valid since `type` accepts
                // a single parameter. This likely is a confusion with `type[a | b]` or
                // `type[Union[a, b]]`. Do not emit a diagnostic for invalid type
                // annotations.
                if !matches!(**slice, Expr::Tuple(_)) && semantic.match_builtin_expr(value, "type")
                {
                    type_exprs.push(slice);
                } else {
                    other_exprs.push(expr);
                }
            }
            _ => other_exprs.push(expr),
        }
    };

    traverse_union(&mut collect_type_exprs, semantic, union);

    // Return if zero or one `type` expressions are found.
    if type_exprs.len() <= 1 {
        return;
    }

    let type_members: Vec<Name> = type_exprs
        .iter()
        .map(|type_expr| Name::new(checker.locator().slice(type_expr)))
        .collect();

    let mut diagnostic = Diagnostic::new(
        UnnecessaryTypeUnion {
            members: type_members.clone(),
            union_kind,
        },
        union.range(),
    );

    if semantic.has_builtin_binding("type") {
        // Construct the content for the [`Fix`] based on if we encountered a PEP604 union.
        let content = match union_kind {
            UnionKind::PEP604 => {
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
            }
            UnionKind::TypingUnion => {
                // When subscript is None, it uses the previous match case.
                let subscript = subscript.unwrap();
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
            }
        };

        // Mark [`Fix`] as unsafe when comments are in range.
        let applicability = if checker.comment_ranges().intersects(union.range()) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        };

        diagnostic.set_fix(Fix::applicable_edit(
            Edit::range_replacement(content, union.range()),
            applicability,
        ));
    }

    checker.report_diagnostic(diagnostic);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnionKind {
    /// E.g., `typing.Union[int, str]`
    TypingUnion,
    /// E.g., `int | str`
    PEP604,
}
