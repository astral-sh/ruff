use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct RaiseNotImplemented;

impl AlwaysAutofixableViolation for RaiseNotImplemented {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`raise NotImplemented` should be `raise NotImplementedError`")
    }

    fn autofix_title(&self) -> String {
        "Use `raise NotImplementedError`".to_string()
    }
}

fn match_not_implemented(expr: &Expr) -> Option<&Expr> {
    match &expr.node {
        ExprKind::Call { func, .. } => {
            if let ExprKind::Name { id, .. } = &func.node {
                if id == "NotImplemented" {
                    return Some(func);
                }
            }
        }
        ExprKind::Name { id, .. } => {
            if id == "NotImplemented" {
                return Some(expr);
            }
        }
        _ => {}
    }
    None
}

/// F901
pub fn raise_not_implemented(checker: &mut Checker, expr: &Expr) {
    let Some(expr) = match_not_implemented(expr) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(RaiseNotImplemented, Range::from(expr));
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            "NotImplementedError".to_string(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
