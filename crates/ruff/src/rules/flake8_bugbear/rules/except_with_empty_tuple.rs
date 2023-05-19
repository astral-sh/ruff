use rustpython_parser::ast::{self, Ranged};
use rustpython_parser::ast::{Excepthandler, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct ExceptWithEmptyTuple;

impl Violation for ExceptWithEmptyTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `except ():` with an empty tuple does not catch anything; add exceptions to handle")
    }
}

/// B029
pub(crate) fn except_with_empty_tuple(checker: &mut Checker, excepthandler: &Excepthandler) {
    let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { type_, .. }) = excepthandler;
    let Some(type_) = type_ else {
        return;
    };
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = type_.as_ref() else {
        return;
    };
    if elts.is_empty() {
        checker
            .diagnostics
            .push(Diagnostic::new(ExceptWithEmptyTuple, excepthandler.range()));
    }
}
