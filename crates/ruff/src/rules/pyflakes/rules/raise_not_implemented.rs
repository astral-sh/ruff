use rustpython_parser::ast::{self, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

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
        ExprKind::Call(ast::ExprCall { func, .. }) => {
            if let ExprKind::Name(ast::ExprName { id, .. }) = &func.node {
                if id == "NotImplemented" {
                    return Some(func);
                }
            }
        }
        ExprKind::Name(ast::ExprName { id, .. }) => {
            if id == "NotImplemented" {
                return Some(expr);
            }
        }
        _ => {}
    }
    None
}

/// F901
pub(crate) fn raise_not_implemented(checker: &mut Checker, expr: &Expr) {
    let Some(expr) = match_not_implemented(expr) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(RaiseNotImplemented, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            "NotImplementedError".to_string(),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
