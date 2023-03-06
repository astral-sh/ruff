use ruff_macros::{derive_message_formats, violation};
use rustpython_parser::ast::Excepthandler;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

use rustpython_parser::ast::{ExcepthandlerKind, ExprKind};

#[violation]
pub struct ExceptWithEmptyTuple;

impl Violation for ExceptWithEmptyTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using except (): with an empty tuple does not handle/catch anything. Add exceptions to handle.")
    }
}

/// B029
pub fn except_with_empty_tuple(checker: &mut Checker, excepthandler: &Excepthandler) {
    let ExcepthandlerKind::ExceptHandler { type_, .. } = &excepthandler.node;
    if type_.is_none() {
        return;
    }
    let ExprKind::Tuple { elts, .. } = &type_.as_ref().unwrap().node else {
        return;
    };
    if elts.is_empty() {
        checker.diagnostics.push(Diagnostic::new(
            ExceptWithEmptyTuple,
            Range::from_located(excepthandler),
        ));
    }
}
