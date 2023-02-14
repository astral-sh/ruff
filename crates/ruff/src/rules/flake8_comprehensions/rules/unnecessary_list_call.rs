use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use super::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::flake8_comprehensions::fixes;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct UnnecessaryListCall;
);
impl AlwaysAutofixableViolation for UnnecessaryListCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `list` call (remove the outer call to `list()`)")
    }

    fn autofix_title(&self) -> String {
        "Remove outer `list` call".to_string()
    }
}

/// C411
pub fn unnecessary_list_call(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    let Some(argument) = helpers::first_argument_with_matching_function("list", func, args) else {
        return;
    };
    if !checker.is_builtin("list") {
        return;
    }
    if !matches!(argument, ExprKind::ListComp { .. }) {
        return;
    }
    let mut diagnostic = Diagnostic::new(UnnecessaryListCall, Range::from_located(expr));
    if checker.patch(diagnostic.kind.rule()) {
        match fixes::fix_unnecessary_list_call(checker.locator, checker.stylist, expr) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}
