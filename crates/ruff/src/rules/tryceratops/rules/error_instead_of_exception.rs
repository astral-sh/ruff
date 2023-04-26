use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::Visitor;

use crate::checkers::ast::Checker;
use crate::rules::tryceratops::helpers::LoggerCandidateVisitor;

/// ## What it does
/// Checks for uses of `logging.error` instead of `logging.exception` when
/// logging an exception.
///
/// ## Why is this bad?
/// `logging.exception` logs the exception and the traceback, while
/// `logging.error` only logs the exception. The former is more appropriate
/// when logging an exception, as the traceback is often useful for debugging.
///
/// ## Example
/// ```python
/// import logging
///
///
/// def foo():
///     try:
///         raise NotImplementedError
///     except NotImplementedError:
///         logging.error("Exception occurred")
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
///
/// def foo():
///     try:
///         raise NotImplementedError
///     except NotImplementedError as exc:
///         logging.exception("Exception occurred")
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/library/logging.html#logging.exception)
#[violation]
pub struct ErrorInsteadOfException;

impl Violation for ErrorInsteadOfException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `logging.exception` instead of `logging.error`")
    }
}

/// TRY400
pub fn error_instead_of_exception(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
        let calls = {
            let mut visitor = LoggerCandidateVisitor::new(&checker.ctx);
            visitor.visit_body(body);
            visitor.calls
        };
        for (expr, func) in calls {
            if let ExprKind::Attribute { attr, .. } = &func.node {
                if attr == "error" {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(ErrorInsteadOfException, expr.range()));
                }
            }
        }
    }
}
