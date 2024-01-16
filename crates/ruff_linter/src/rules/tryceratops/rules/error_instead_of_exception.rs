use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, ExceptHandler, Expr};
use ruff_python_semantic::analyze::logging::exc_info;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
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
/// def func():
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
/// def func():
///     try:
///         raise NotImplementedError
///     except NotImplementedError:
///         logging.exception("Exception occurred")
/// ```
///
/// ## References
/// - [Python documentation: `logging.exception`](https://docs.python.org/3/library/logging.html#logging.exception)
#[violation]
pub struct ErrorInsteadOfException;

impl Violation for ErrorInsteadOfException {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `logging.exception` instead of `logging.error`")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with `exception`"))
    }
}

/// TRY400
pub(crate) fn error_instead_of_exception(checker: &mut Checker, handlers: &[ExceptHandler]) {
    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, .. }) = handler;
        let calls = {
            let mut visitor =
                LoggerCandidateVisitor::new(checker.semantic(), &checker.settings.logger_objects);
            visitor.visit_body(body);
            visitor.calls
        };
        for (expr, logging_level) in calls {
            if matches!(logging_level, LoggingLevel::Error) {
                if exc_info(&expr.arguments, checker.semantic()).is_none() {
                    let mut diagnostic = Diagnostic::new(ErrorInsteadOfException, expr.range());
                    if checker.settings.preview.is_enabled() {
                        match expr.func.as_ref() {
                            Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
                                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                                    "exception".to_string(),
                                    attr.range(),
                                )));
                            }
                            Expr::Name(_) => {
                                diagnostic.try_set_fix(|| {
                                    let (import_edit, binding) =
                                        checker.importer().get_or_import_symbol(
                                            &ImportRequest::import("logging", "exception"),
                                            expr.start(),
                                            checker.semantic(),
                                        )?;
                                    let name_edit =
                                        Edit::range_replacement(binding, expr.func.range());
                                    Ok(Fix::safe_edits(import_edit, [name_edit]))
                                });
                            }
                            _ => {}
                        }
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}
