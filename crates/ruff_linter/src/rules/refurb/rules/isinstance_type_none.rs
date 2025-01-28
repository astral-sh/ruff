use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_python_ast::{self as ast, CmpOp, Expr, Operator};

use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

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
/// ## References
/// - [Python documentation: `isinstance`](https://docs.python.org/3/library/functions.html#isinstance)
/// - [Python documentation: `None`](https://docs.python.org/3/library/constants.html#None)
/// - [Python documentation: `type`](https://docs.python.org/3/library/functions.html#type)
/// - [Python documentation: Identity comparisons](https://docs.python.org/3/reference/expressions.html#is-not)
#[derive(ViolationMetadata)]
pub(crate) struct IsinstanceTypeNone;

impl Violation for IsinstanceTypeNone {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Prefer `is` operator over `isinstance` to check if an object is `None`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `is` operator".to_string())
    }
}

/// FURB168
pub(crate) fn isinstance_type_none(checker: &mut Checker, call: &ast::ExprCall) {
    let semantic = checker.semantic();
    let (func, arguments) = (&call.func, &call.arguments);

    if !arguments.keywords.is_empty() {
        return;
    }

    let [expr, types] = arguments.args.as_ref() else {
        return;
    };

    if !semantic.match_builtin_expr(func, "isinstance") {
        return;
    }

    if !is_none(types, semantic) {
        return;
    }

    let fix = replace_with_identity_check(expr, call.range, checker);
    let diagnostic = Diagnostic::new(IsinstanceTypeNone, call.range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

/// Returns `true` if the given expression is equivalent to checking if the
/// object type is `None` when used with the `isinstance` builtin.
fn is_none(expr: &Expr, semantic: &SemanticModel) -> bool {
    fn inner(expr: &Expr, in_union_context: bool, semantic: &SemanticModel) -> bool {
        match expr {
            // Ex) `None`
            // Note: `isinstance` only accepts `None` as a type when used with
            // the union operator, so we need to check if we're in a union.
            Expr::NoneLiteral(_) if in_union_context => true,

            // Ex) `type(None)`
            Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) => {
                if !semantic.match_builtin_expr(func, "type") {
                    return false;
                }

                if !arguments.keywords.is_empty() {
                    return false;
                }

                matches!(arguments.args.as_ref(), [Expr::NoneLiteral(_)])
            }

            // Ex) `(type(None),)`
            Expr::Tuple(tuple) => tuple.iter().all(|element| inner(element, false, semantic)),

            // Ex) `type(None) | type(None)`
            Expr::BinOp(ast::ExprBinOp {
                left,
                op: Operator::BitOr,
                right,
                ..
            }) => {
                // `None | None` is a `TypeError` at runtime
                if left.is_none_literal_expr() && right.is_none_literal_expr() {
                    return false;
                }

                inner(left, true, semantic) && inner(right, true, semantic)
            }

            // Ex) `Union[None, ...]`
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                if !semantic.match_typing_expr(value, "Union") {
                    return false;
                }

                match slice.as_ref() {
                    Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                        elts.iter().all(|element| inner(element, true, semantic))
                    }
                    slice => inner(slice, true, semantic),
                }
            }

            // Otherwise, return false.
            _ => false,
        }
    }
    inner(expr, false, semantic)
}

fn replace_with_identity_check(expr: &Expr, range: TextRange, checker: &Checker) -> Fix {
    let (semantic, generator) = (checker.semantic(), checker.generator());

    let new_expr = Expr::Compare(ast::ExprCompare {
        left: expr.clone().into(),
        ops: [CmpOp::Is].into(),
        comparators: [ast::ExprNoneLiteral::default().into()].into(),
        range: TextRange::default(),
    });

    let new_content = generator.expr(&new_expr);
    let new_content = if semantic.current_expression_parent().is_some() {
        format!("({new_content})")
    } else {
        new_content
    };

    let applicability = if checker.comment_ranges().intersects(range) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    let edit = Edit::range_replacement(new_content, range);

    Fix::applicable_edit(edit, applicability)
}
