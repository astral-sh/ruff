use rustpython_parser::ast::Excepthandler;
use rustpython_parser::ast::{ExcepthandlerKind, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

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
pub fn except_with_empty_tuple(checker: &mut Checker, excepthandler: &Excepthandler) {
    let ExcepthandlerKind::ExceptHandler { type_, .. } = &excepthandler.node;
    let Some(type_) = type_ else {
        return;
    };
    let ExprKind::Tuple { elts, .. } = &type_.node else {
        return;
    };
    if elts.is_empty() {
        checker.diagnostics.push(Diagnostic::new(
            ExceptWithEmptyTuple,
            Range::from(excepthandler),
        ));
    }
}
