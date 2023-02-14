use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::helpers::match_parens;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct UnnecessaryParenOnRaiseException;
);
impl AlwaysAutofixableViolation for UnnecessaryParenOnRaiseException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary parentheses on raised exception")
    }

    fn autofix_title(&self) -> String {
        format!("Remove unnecessary parentheses")
    }
}

/// RSE102
pub fn unnecessary_paren_on_raise_exception(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Call {
        func,
        args,
        keywords,
    } = &expr.node
    {
        if args.is_empty() && keywords.is_empty() {
            let range = match_parens(func.end_location.unwrap(), checker.locator)
                .expect("Expected call to include parentheses");
            let mut diagnostic = Diagnostic::new(UnnecessaryParenOnRaiseException, range);
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.amend(Fix::deletion(
                    func.end_location.unwrap(),
                    range.end_location,
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
