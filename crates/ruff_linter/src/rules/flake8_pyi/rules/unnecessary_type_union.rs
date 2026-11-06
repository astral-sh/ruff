use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::Name;
use ruff_python_ast::{Builder, Expr, ExprKind};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::Ranged;

use super::union_with_replacement;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::syntax;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

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
#[violation_metadata(stable_since = "v0.0.283", category = Category::Complexity)]
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
pub(crate) fn unnecessary_type_union<'a>(checker: &Checker, &union: &Expr<'a>) {
    let semantic = checker.semantic();

    // The `|` operator isn't always safe to allow to runtime-evaluated annotations.
    if semantic.execution_context().is_runtime() {
        return;
    }

    // Check if `union` is a PEP604 union (e.g. `float | int`) or a `typing.Union[float, int]`
    let mut union_subscript = union.as_subscript_expr();
    if union_subscript
        .is_some_and(|subscript| !semantic.match_typing_expr(subscript.value(), "Union"))
    {
        return;
    }
    let mut type_exprs: Vec<Expr<'_>> = Vec::new();
    let mut other_exprs: Vec<Expr<'_>> = Vec::new();

    let mut collect_type_exprs = |expr: Expr<'a>, parent: Expr<'a>| {
        // If a PEP604-style union is used within a `typing.Union`, then the fix can
        // use PEP604-style unions.
        if matches!(parent.classify(), ExprKind::BinOp(_)) {
            union_subscript = None;
        }
        match expr.classify() {
            ExprKind::Subscript(expr_subscript) => {
                let slice = expr_subscript.slice();
                let value = expr_subscript.value();

                // The annotation `type[a, b]` is not valid since `type` accepts
                // a single parameter. This likely is a confusion with `type[a | b]` or
                // `type[Union[a, b]]`. Do not emit a diagnostic for invalid type
                // annotations.
                if !matches!(slice.classify(), ExprKind::Tuple(_))
                    && semantic.match_builtin_expr(value, "type")
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

    let mut diagnostic = checker.report_diagnostic(
        UnnecessaryTypeUnion {
            members: type_members.clone(),
            union_kind: if union_subscript.is_some() {
                UnionKind::TypingUnion
            } else {
                UnionKind::PEP604
            },
        },
        union.range(),
    );

    if semantic.has_builtin_binding("type") {
        let mut builder = Builder::default();
        let type_name = syntax::name(&mut builder, "type");
        let slice = if let Some(subscript) = union_subscript {
            let value = builder.copy_node(subscript.value());
            let elts = builder.map_list(type_members, |builder, member| {
                syntax::name(builder, member.as_str())
            });
            syntax::subscript_tuple(&mut builder, value, elts)
        } else {
            syntax::pep_604_union(&mut builder, type_exprs)
        };
        let types = syntax::subscript(&mut builder, type_name, slice);
        let replacement = union_with_replacement(&mut builder, types, other_exprs, union_subscript);
        let content = checker.generator().expr(builder.get(replacement));
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnionKind {
    /// E.g., `typing.Union[int, str]`
    TypingUnion,
    /// E.g., `int | str`
    PEP604,
}
