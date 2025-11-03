use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableExpr;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for useless `if`-`else` conditions with identical arms.
///
/// ## Why is this bad?
/// Useless `if`-`else` conditions add unnecessary complexity to the code without
/// providing any logical benefit. Assigning the value directly is clearer.
///
/// ## Example
/// ```python
/// foo = x if y else x
/// ```
///
/// Use instead:
/// ```python
/// foo = x
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.9.0")]
pub(crate) struct UselessIfElse;

impl Violation for UselessIfElse {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Useless `if`-`else` condition".to_string()
    }
}

/// RUF034
pub(crate) fn useless_if_else(checker: &Checker, if_expr: &ast::ExprIf) {
    let ast::ExprIf {
        body,
        orelse,
        range,
        ..
    } = if_expr;

    // Skip if the `body` and `orelse` are not the same.
    if ComparableExpr::from(body) != ComparableExpr::from(orelse) {
        return;
    }

    checker.report_diagnostic(UselessIfElse, *range);
}
