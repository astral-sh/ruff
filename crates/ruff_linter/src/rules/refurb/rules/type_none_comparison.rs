use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;
use crate::rules::refurb::helpers::generate_none_identity_comparison;

/// ## What it does
/// Checks for uses of `type` that compare the type of an object to the type of
/// `None`.
///
/// ## Why is this bad?
/// There is only ever one instance of `None`, so it is more efficient and
/// readable to use the `is` operator to check if an object is `None`.
///
/// ## Example
/// ```python
/// type(obj) is type(None)
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
pub struct TypeNoneComparison {
    object: Name,
    comparison: Comparison,
}

impl Violation for TypeNoneComparison {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeNoneComparison { object, .. } = self;
        format!("Compare the identities of `{object}` and `None` instead of their respective types")
    }

    fn fix_title(&self) -> Option<String> {
        let TypeNoneComparison { object, comparison } = self;
        match comparison {
            Comparison::Is | Comparison::Eq => Some(format!("Replace with `{object} is None`")),
            Comparison::IsNot | Comparison::NotEq => {
                Some(format!("Replace with `{object} is not None`"))
            }
        }
    }
}

/// FURB169
pub(crate) fn type_none_comparison(checker: &mut Checker, compare: &ast::ExprCompare) {
    let ([op], [right]) = (&*compare.ops, &*compare.comparators) else {
        return;
    };

    // Ensure that the comparison is an identity or equality test.
    let comparison = match op {
        CmpOp::Is => Comparison::Is,
        CmpOp::IsNot => Comparison::IsNot,
        CmpOp::Eq => Comparison::Eq,
        CmpOp::NotEq => Comparison::NotEq,
        _ => return,
    };

    // Get the objects whose types are being compared.
    let Some(left_arg) = type_call_arg(&compare.left, checker.semantic()) else {
        return;
    };
    let Some(right_arg) = type_call_arg(right, checker.semantic()) else {
        return;
    };

    // If one of the objects is `None`, get the other object; else, return.
    let other_arg = match (
        left_arg.is_none_literal_expr(),
        right_arg.is_none_literal_expr(),
    ) {
        (true, false) => right_arg,
        (false, true) => left_arg,
        // If both are `None`, just pick one.
        (true, true) => left_arg,
        _ => return,
    };

    // Get the name of the other object (or `None` if both were `None`).
    let other_arg_name = match other_arg {
        Expr::Name(ast::ExprName { id, .. }) => id.clone(),
        Expr::NoneLiteral { .. } => Name::new_static("None"),
        _ => return,
    };

    let mut diagnostic = Diagnostic::new(
        TypeNoneComparison {
            object: other_arg_name.clone(),
            comparison,
        },
        compare.range(),
    );
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        pad(
            match comparison {
                Comparison::Is | Comparison::Eq => {
                    generate_none_identity_comparison(other_arg_name, false, checker.generator())
                }
                Comparison::IsNot | Comparison::NotEq => {
                    generate_none_identity_comparison(other_arg_name, true, checker.generator())
                }
            },
            compare.range(),
            checker.locator(),
        ),
        compare.range(),
    )));
    checker.diagnostics.push(diagnostic);
}

/// Returns the object passed to the function, if the expression is a call to
/// `type` with a single argument.
fn type_call_arg<'a>(expr: &'a Expr, semantic: &'a SemanticModel) -> Option<&'a Expr> {
    // The expression must be a single-argument call to `type`.
    let ast::ExprCall {
        func, arguments, ..
    } = expr.as_call_expr()?;
    if arguments.len() != 1 {
        return None;
    }
    // The function itself must be the builtin `type`.
    if !semantic.match_builtin_expr(func, "type") {
        return None;
    }
    arguments.find_positional(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Comparison {
    Is,
    IsNot,
    Eq,
    NotEq,
}
