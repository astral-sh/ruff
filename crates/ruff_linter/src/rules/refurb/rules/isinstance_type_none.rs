use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_python_ast::{self as ast, Expr, Operator};

use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;
use crate::rules::refurb::helpers::generate_none_identity_comparison;

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
#[violation]
pub struct IsinstanceTypeNone;

impl Violation for IsinstanceTypeNone {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `is` operator over `isinstance` to check if an object is `None`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `is` operator".to_string())
    }
}

/// FURB168
pub(crate) fn isinstance_type_none(checker: &mut Checker, call: &ast::ExprCall) {
    let Some(types) = call.arguments.find_positional(1) else {
        return;
    };
    if !checker
        .semantic()
        .match_builtin_expr(&call.func, "isinstance")
    {
        return;
    }
    if is_none(types) {
        let Some(Expr::Name(ast::ExprName {
            id: object_name, ..
        })) = call.arguments.find_positional(0)
        else {
            return;
        };
        let mut diagnostic = Diagnostic::new(IsinstanceTypeNone, call.range());
        let replacement =
            generate_none_identity_comparison(object_name.clone(), false, checker.generator());
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            pad(replacement, call.range(), checker.locator()),
            call.range(),
        )));
        checker.diagnostics.push(diagnostic);
    }
}

/// Returns `true` if the given expression is equivalent to checking if the
/// object type is `None` when used with the `isinstance` builtin.
fn is_none(expr: &Expr) -> bool {
    fn inner(expr: &Expr, in_union_context: bool) -> bool {
        match expr {
            // Ex) `None`
            // Note: `isinstance` only accepts `None` as a type when used with
            // the union operator, so we need to check if we're in a union.
            Expr::NoneLiteral(_) if in_union_context => true,

            // Ex) `type(None)`
            Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) if arguments.len() == 1 => {
                if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                    if id.as_str() == "type" {
                        return matches!(arguments.args.first(), Some(Expr::NoneLiteral(_)));
                    }
                }
                false
            }

            // Ex) `(type(None),)`
            Expr::Tuple(tuple) => tuple.iter().all(|element| inner(element, false)),

            // Ex) `type(None) | type(None)`
            Expr::BinOp(ast::ExprBinOp {
                left,
                op: Operator::BitOr,
                right,
                ..
            }) => inner(left, true) && inner(right, true),

            // Otherwise, return false.
            _ => false,
        }
    }
    inner(expr, false)
}
