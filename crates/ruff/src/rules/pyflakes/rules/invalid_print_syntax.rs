use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[violation]
pub struct InvalidPrintSyntax;

impl Violation for InvalidPrintSyntax {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `>>` is invalid with `print` function")
    }
}

/// F633
pub fn invalid_print_syntax(checker: &mut Checker, left: &Expr) {
    let ExprKind::Name { id, .. } = &left.node else {
        return;
    };
    if id != "print" {
        return;
    }
    if !checker.ctx.is_builtin("print") {
        return;
    };
    checker
        .diagnostics
        .push(Diagnostic::new(InvalidPrintSyntax, Range::from(left)));
}
