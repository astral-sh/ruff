use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

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
pub(crate) fn invalid_print_syntax(checker: &mut Checker, left: &Expr) {
    let Expr::Name(ast::ExprName { id, .. }) = &left else {
        return;
    };
    if id != "print" {
        return;
    }
    if !checker.semantic_model().is_builtin("print") {
        return;
    };
    checker
        .diagnostics
        .push(Diagnostic::new(InvalidPrintSyntax, left.range()));
}
