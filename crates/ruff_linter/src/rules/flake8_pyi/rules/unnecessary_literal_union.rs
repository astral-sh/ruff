use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Builder, Expr, ExprKind};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::Ranged;

use super::union_with_replacement;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::syntax;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for the presence of multiple literal types in a union.
///
/// ## Why is this bad?
/// `Literal["foo", 42]` has identical semantics to
/// `Literal["foo"] | Literal[42]`, but is clearer and more concise.
///
/// ## Example
/// ```pyi
/// from typing import Literal
///
/// field: Literal[1] | Literal[2] | str
/// ```
///
/// Use instead:
/// ```pyi
/// from typing import Literal
///
/// field: Literal[1, 2] | str
/// ```
///
/// ## Fix safety
/// This fix is marked unsafe if it would delete any comments within the replacement range.
///
/// An example to illustrate where comments are preserved and where they are not:
///
/// ```pyi
/// from typing import Literal
///
/// field: (
///     # deleted comment
///     Literal["a", "b"]  # deleted comment
///     # deleted comment
///     | Literal["c", "d"]  # preserved comment
/// )
/// ```
///
/// ## References
/// - [Python documentation: `typing.Literal`](https://docs.python.org/3/library/typing.html#typing.Literal)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.278", category = Category::Complexity)]
pub(crate) struct UnnecessaryLiteralUnion {
    members: Vec<String>,
}

impl Violation for UnnecessaryLiteralUnion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Multiple literal members in a union. Use a single literal, e.g. `Literal[{}]`",
            self.members.join(", ")
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with a single `Literal`".to_string())
    }
}

/// PYI030
pub(crate) fn unnecessary_literal_union<'a>(checker: &Checker, &expr: &Expr<'a>) {
    let mut literal_exprs = Vec::new();
    let mut other_exprs = Vec::new();

    // For the sake of consistency, use the first `Literal` subscript to construct the fix.
    let mut literal_subscript = None;
    let mut total_literals = 0;

    // Split members into `literal_exprs` if they are a `Literal` annotation  and `other_exprs` otherwise
    let mut collect_literal_expr = |expr: Expr<'a>, _parent: Expr<'a>| {
        if let ExprKind::Subscript(subscript_node) = expr.classify() {
            let value = subscript_node.value();
            let slice = subscript_node.slice();
            if checker.semantic().match_typing_expr(value, "Literal") {
                total_literals += 1;

                if literal_subscript.is_none() {
                    literal_subscript = Some(value);
                }

                // flatten already-unioned literals to later union again
                if let ExprKind::Tuple(tuple) = slice.classify() {
                    for item in tuple {
                        literal_exprs.push(item);
                    }
                } else {
                    literal_exprs.push(slice);
                }
            } else {
                other_exprs.push(expr);
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
            .match_typing_expr(subscript.value(), "Union")
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

    let mut diagnostic = checker.report_diagnostic(
        UnnecessaryLiteralUnion {
            members: literal_exprs
                .iter()
                .map(|expr| checker.locator().slice(expr).to_string())
                .collect(),
        },
        expr.range(),
    );

    diagnostic.set_fix({
        let mut builder = Builder::default();
        let value = builder.copy_node(literal_subscript);
        let elts = builder.copy_nodes(literal_exprs);
        let literal = syntax::subscript_tuple(&mut builder, value, elts);
        let replacement =
            union_with_replacement(&mut builder, literal, other_exprs, union_subscript);
        let edit = Edit::range_replacement(
            checker.generator().expr(builder.get(replacement)),
            expr.range(),
        );
        if checker.comment_ranges().intersects(expr.range()) {
            Fix::unsafe_edit(edit)
        } else {
            Fix::safe_edit(edit)
        }
    });
}
