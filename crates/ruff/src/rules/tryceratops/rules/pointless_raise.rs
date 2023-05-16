

use rustpython_parser::ast::{self, Excepthandler, StmtKind, ExcepthandlerKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};


use crate::checkers::ast::Checker;


/// ## What it does
/// Checks for uses of `raise` directly after a `rescue`
///
/// ## Why is this bad?
/// Catching an error just to reraise it is pointless. Instead, remove error-handling and let the error propogate naturally
///
/// ## Example
/// ```python
///
/// def foo():
///     try:
///         bar()
///     except NotImplementedError:
//          raise
/// ```
///
/// Use instead:
/// ```python
///
/// def foo():
///     bar()
/// ```
#[violation]
pub struct PointlessRaise;

impl Violation for PointlessRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Remove this exception handler as the error is immediately re-raised")
    }
}

/// TRY302
pub(crate) fn pointless_raise(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler(handler) = &handler.node;
        let body = &handler.body;

        if let Some(stmt) = body.first() {
            if let StmtKind::Raise(ast::StmtRaise { exc: None, .. }) = &stmt.node {
                checker
                    .diagnostics
                    .push(Diagnostic::new(PointlessRaise, stmt.range()));
            }
        }
    }
}
