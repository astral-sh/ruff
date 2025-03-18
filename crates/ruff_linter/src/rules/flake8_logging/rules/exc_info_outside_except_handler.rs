use ruff_diagnostics::{Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::Truthiness;
use ruff_python_ast::{Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::analyze::logging::is_logger_candidate;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};
use crate::rules::flake8_logging::rules::helpers::outside_handlers;

/// ## What it does
/// Checks for logging calls with `exc_info=` outside exception handlers.
///
/// ## Why is this bad?
/// Using `exc_info=True` outside of an exception handler
/// attaches `None` as the exception information, leading to confusing messages:
///
/// ```pycon
/// >>> logging.warning("Uh oh", exc_info=True)
/// WARNING:root:Uh oh
/// NoneType: None
/// ```
///
/// ## Example
///
/// ```python
/// import logging
///
///
/// logging.warning("Foobar", exc_info=True)
/// ```
///
/// Use instead:
///
/// ```python
/// import logging
///
///
/// logging.warning("Foobar")
/// ```
///
/// ## Fix safety
/// The fix is always marked as unsafe, as it changes runtime behavior.
#[derive(ViolationMetadata)]
pub(crate) struct ExcInfoOutsideExceptHandler;

impl Violation for ExcInfoOutsideExceptHandler {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`exc_info=` outside exception handlers".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove `exc_info=`".to_string())
    }
}

pub(crate) fn exc_info_outside_except_handler(checker: &Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    if !outside_handlers(call.start(), semantic) {
        return;
    }

    match &*call.func {
        func @ Expr::Attribute(ExprAttribute { attr, .. }) => {
            if !is_logger_candidate(func, semantic, &checker.settings.logger_objects) {
                return;
            }

            if LoggingLevel::from_attribute(attr).is_none() {
                return;
            }
        }

        func @ Expr::Name(_) => {
            let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
                return;
            };

            let ["logging", attr] = qualified_name.segments() else {
                return;
            };

            if *attr != "log" && LoggingLevel::from_attribute(attr).is_none() {
                return;
            }
        }

        _ => return,
    }

    let Some(exc_info) = call.arguments.find_keyword("exc_info") else {
        return;
    };

    let truthiness = Truthiness::from_expr(&exc_info.value, |id| semantic.has_builtin_binding(id));

    if truthiness.into_bool() != Some(true) {
        return;
    }

    let arguments = &call.arguments;
    let source = checker.source();

    let mut diagnostic = Diagnostic::new(ExcInfoOutsideExceptHandler, exc_info.range);

    diagnostic.try_set_fix(|| {
        let edit = remove_argument(exc_info, arguments, Parentheses::Preserve, source)?;
        Ok(Fix::unsafe_edit(edit))
    });

    checker.report_diagnostic(diagnostic);
}
