use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, Ranged};

use crate::checkers::ast::Checker;

#[violation]
pub struct UnusedConditionalExpressionResult;

impl Violation for UnusedConditionalExpressionResult {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Conditional expression with unused result; use an if-else statement instead")
    }
}

pub(crate) fn unused_conditional_expression_result(checker: &mut Checker, expr: &Expr) {
    if let Expr::IfExp(_) = expr {
        let diagnostic = Diagnostic::new(UnusedConditionalExpressionResult, expr.range());

        checker.diagnostics.push(diagnostic);
    }
}
