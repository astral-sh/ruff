use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprKind, Operator};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::refurb::helpers::replace_with_identity_check;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `isinstance` that check if an object is of type `None`.
///
/// ## Why is this bad?
/// There is only ever one instance of `None`, so it is more efficient and
/// readable to use the `is` operator to check if an object is `None`.
///
/// ## Example
/// ```python
/// isinstance(obj, type(None))
/// ```
///
/// Use instead:
/// ```python
/// obj is None
/// ```
///
/// ## Fix safety
/// The fix will be marked as unsafe if there are any comments within the call.
///
/// ## References
/// - [Python documentation: `isinstance`](https://docs.python.org/3/library/functions.html#isinstance)
/// - [Python documentation: `None`](https://docs.python.org/3/library/constants.html#None)
/// - [Python documentation: `type`](https://docs.python.org/3/library/functions.html#type)
/// - [Python documentation: Identity comparisons](https://docs.python.org/3/reference/expressions.html#is-not)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.5.0", category = Category::Complexity)]
pub(crate) struct IsinstanceTypeNone;

impl Violation for IsinstanceTypeNone {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Prefer `is` operator over `isinstance` to check if an object is `None`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `is` operator".to_string())
    }
}

/// FURB168
pub(crate) fn isinstance_type_none(checker: &Checker, call: ast::ExprCall<'_>) {
    let semantic = checker.semantic();
    let (func, arguments) = (call.func(), call.arguments());

    if !arguments.keywords().is_empty() {
        return;
    }

    let Some([expr, types]) = arguments.args().as_array() else {
        return;
    };

    if !semantic.match_builtin_expr(func, "isinstance") {
        return;
    }

    if !is_none(types, semantic) {
        return;
    }

    let fix = replace_with_identity_check(expr, call.range(), false, checker);
    checker
        .report_diagnostic(IsinstanceTypeNone, call.range())
        .set_fix(fix);
}

/// Returns `true` if the given expression is equivalent to checking if the
/// object type is `None` when used with the `isinstance` builtin.
fn is_none(expr: Expr<'_>, semantic: &SemanticModel) -> bool {
    fn inner(expr: Expr<'_>, in_union_context: bool, semantic: &SemanticModel) -> bool {
        match expr.classify() {
            // Ex) `None`
            // Note: `isinstance` only accepts `None` as a type when used with
            // the union operator, so we need to check if we're in a union.
            ExprKind::NoneLiteral(_) if in_union_context => true,

            // Ex) `type(None)`
            ExprKind::Call(expr_call) => {
                let func = expr_call.func();
                let arguments = expr_call.arguments();

                if !semantic.match_builtin_expr(func, "type") {
                    return false;
                }

                if !arguments.keywords().is_empty() {
                    return false;
                }

                matches!(
                    arguments
                        .args()
                        .as_array()
                        .map(|items| items.map(Expr::classify)),
                    Some([ExprKind::NoneLiteral(_)])
                )
            }

            // Ex) `(type(None),)`
            ExprKind::Tuple(tuple) => {
                !tuple.elts().is_empty()
                    && tuple.iter().all(|element| inner(element, false, semantic))
            }

            // Ex) `type(None) | type(None)`
            ExprKind::BinOp(expr_bin_op) if expr_bin_op.op() == Operator::BitOr => {
                let left = expr_bin_op.left();
                let right = expr_bin_op.right();

                // `None | None` is a `TypeError` at runtime
                if left.is_none_literal_expr() && right.is_none_literal_expr() {
                    return false;
                }

                inner(left, true, semantic) && inner(right, true, semantic)
            }

            // Ex) `Union[None, ...]`
            ExprKind::Subscript(expr_subscript) => {
                let value = expr_subscript.value();
                let slice = expr_subscript.slice();

                if !semantic.match_typing_expr(value, "Union") {
                    return false;
                }

                match slice.fields() {
                    ast::ExprFields::Tuple(ast::ExprTupleFields { elts, .. }) => {
                        !elts.is_empty()
                            && elts.iter().all(|element| inner(element, true, semantic))
                    }
                    slice => inner(slice.view(), true, semantic),
                }
            }

            // Otherwise, return false.
            _ => false,
        }
    }
    inner(expr, false, semantic)
}
