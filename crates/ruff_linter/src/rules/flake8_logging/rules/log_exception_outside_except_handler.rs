use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::analyze::logging;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_logging::rules::helpers::outside_handlers;

/// ## What it does
/// Checks for `.exception()` logging calls outside of exception handlers.
///
/// ## Why is this bad?
/// [The documentation] states:
/// > This function should only be called from an exception handler.
///
/// Calling `.exception()` outside of an exception handler
/// attaches `None` as exception information, leading to confusing messages:
///
/// ```pycon
/// >>> logging.exception("example")
/// ERROR:root:example
/// NoneType: None
/// ```
///
/// ## Example
///
/// ```python
/// import logging
///
/// logging.exception("Foobar")
/// ```
///
/// Use instead:
///
/// ```python
/// import logging
///
/// logging.error("Foobar")
/// ```
///
/// ## Fix safety
/// The fix, if available, will always be marked as unsafe, as it changes runtime behavior.
///
/// [The documentation]: https://docs.python.org/3/library/logging.html#logging.exception
#[derive(ViolationMetadata)]
pub(crate) struct LogExceptionOutsideExceptHandler;

impl Violation for LogExceptionOutsideExceptHandler {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`.exception()` call outside exception handlers".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `.error()`".to_string())
    }
}

/// LOG004
pub(crate) fn log_exception_outside_except_handler(checker: &Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    if !outside_handlers(call.start(), semantic) {
        return;
    }

    let fix = match &*call.func {
        func @ Expr::Attribute(ExprAttribute { attr, .. }) => {
            let logger_objects = &checker.settings.logger_objects;

            if !logging::is_logger_candidate(func, semantic, logger_objects) {
                return;
            }

            if attr != "exception" {
                return;
            }

            let edit = Edit::range_replacement("error".to_string(), attr.range);

            Some(Fix::unsafe_edit(edit))
        }

        func @ Expr::Name(_) => {
            let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
                return;
            };

            if !matches!(qualified_name.segments(), ["logging", "exception"]) {
                return;
            }

            None
        }

        _ => return,
    };

    let mut diagnostic = Diagnostic::new(LogExceptionOutsideExceptHandler, call.range);

    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }

    checker.report_diagnostic(diagnostic);
}
