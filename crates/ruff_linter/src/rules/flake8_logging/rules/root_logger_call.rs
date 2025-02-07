use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::ExprCall;
use ruff_python_semantic::Modules;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for usages of the following `logging` top-level functions:
/// `debug`, `info`, `warn`, `warning`, `error`, `critical`, `log`, `exception`.
///
/// ## Why is this bad?
/// Using the root logger causes the messages to have no source information,
/// making them less useful for debugging.
///
/// ## Example
/// ```python
/// import logging
///
/// logging.info("Foobar")
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// logger = logging.getLogger(__name__)
/// logger.info("Foobar")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct RootLoggerCall {
    attr: String,
}

impl Violation for RootLoggerCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`{}()` call on root logger", self.attr)
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use own logger instead".to_string())
    }
}

/// LOG015
pub(crate) fn root_logger_call(checker: &Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::LOGGING) {
        return;
    }

    let Some(qualified_name) = semantic.resolve_qualified_name(&call.func) else {
        return;
    };

    let attr = match qualified_name.segments() {
        ["logging", attr] if is_logger_method_name(attr) => attr,
        _ => return,
    };

    let kind = RootLoggerCall {
        attr: (*attr).to_string(),
    };
    let diagnostic = Diagnostic::new(kind, call.range);

    checker.report_diagnostic(diagnostic);
}

#[inline]
fn is_logger_method_name(attr: &str) -> bool {
    matches!(
        attr,
        "debug" | "info" | "warn" | "warning" | "error" | "critical" | "log" | "exception"
    )
}
