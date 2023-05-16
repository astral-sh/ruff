

use rustpython_parser::ast::{self, Excepthandler, StmtKind, ExcepthandlerKind, Stmt, Attributed, Expr, ExprKind, ExprName, Identifier};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};


use crate::checkers::ast::Checker;


/// ## What it does
/// Checks for uses of `raise` directly after a `rescue`.
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
    let handler_errs = handlers.iter().map(|handler| {
        let ExcepthandlerKind::ExceptHandler(handler) = &handler.node;
        let body = &handler.body;

        // Match if the body consists of a single `raise` statement and nothing else
        if let [stmt @ Stmt { node: StmtKind::Raise(raise), .. }] = body.as_slice() {
            //dbg!(&stmt, &raise);
            //dbg!(&handler);
            match raise.exc.as_ref().map(|e| &e.node) {
                None => Some(Diagnostic::new(PointlessRaise, stmt.range())),
                Some(ExprKind::Name(ExprName { id, .. })) if Some(id) == handler.name.as_ref() => Some(Diagnostic::new(PointlessRaise, stmt.range())),
                _ => None
            }
        } else {
            None
        }
    }).collect::<Vec<_>>();

    if handler_errs.iter().all(Option::is_some) {
        // All handlers have diagnostics
        for err in handler_errs {
            checker.diagnostics.push(err.unwrap());
        }
    }
}
