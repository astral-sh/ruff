use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableExpr;

/// ## What it does
/// Checks for conditional expressions (ternary operators) where both the true
/// and false branches return the same value.
///
/// ## Why is this bad?
/// Redundant conditional expressions add unnecessary complexity to the code without
/// providing any logical benefit.
///
/// Assigning the value directly is clearer and more explicit, and
/// should be preferred.
///
/// ## Example
/// ```python
/// # Bad
/// foo = x if y else x
/// ```
///
/// Use instead:
/// ```python
/// # Good
/// foo = x
/// ```
#[violation]
pub struct RedundantTernary;

impl Violation for RedundantTernary {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Redundant conditional expression")
    }
}

/// RUF102
pub(crate) fn redundant_ternary(checker: &mut Checker, if_expr: &ast::ExprIf) {
    let ast::ExprIf {
        body,
        orelse,
        range,
        ..
    } = if_expr;

    // Skip if the body and orelse are not the same
    if ComparableExpr::from(body) != ComparableExpr::from(orelse) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(RedundantTernary, *range));
}
