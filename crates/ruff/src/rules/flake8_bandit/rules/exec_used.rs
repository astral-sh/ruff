use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct ExecBuiltin;

impl Violation for ExecBuiltin {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `exec` detected")
    }
}

/// S102
pub(crate) fn exec_used(expr: &Expr, func: &Expr) -> Option<Diagnostic> {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        return None;
    };
    if id != "exec" {
        return None;
    }
    Some(Diagnostic::new(ExecBuiltin, expr.range()))
}
