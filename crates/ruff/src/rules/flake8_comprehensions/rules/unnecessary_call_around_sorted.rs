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
    pub struct UnnecessaryCallAroundSorted {
        pub func: String,
    }
);
impl AlwaysAutofixableViolation for UnnecessaryCallAroundSorted {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryCallAroundSorted { func } = self;
        format!("Unnecessary `{func}` call around `sorted()`")
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryCallAroundSorted { func } = self;
        format!("Remove unnecessary `{func}` call")
    }
}

/// C413
pub fn unnecessary_call_around_sorted(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Some(outer) = helpers::function_name(func) else {
        return;
    };
    if !(outer == "list" || outer == "reversed") {
        return;
    }
    let Some(arg) = args.first() else {
        return;
    };
    let ExprKind::Call { func, .. } = &arg.node else {
        return;
    };
    let Some(inner) = helpers::function_name(func) else {
        return;
    };
    if inner != "sorted" {
        return;
    }
    if !checker.is_builtin(inner) || !checker.is_builtin(outer) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryCallAroundSorted {
            func: outer.to_string(),
        },
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        match fixes::fix_unnecessary_call_around_sorted(checker.locator, checker.stylist, expr) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}
