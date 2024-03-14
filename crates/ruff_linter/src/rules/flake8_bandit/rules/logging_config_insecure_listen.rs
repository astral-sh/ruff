use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for insecure `logging.config.listen` calls.
///
/// ## Why is this bad?
/// `logging.config.listen` starts a server that listens for logging
/// configuration requests. This is insecure, as parts of the configuration are
/// passed to the built-in `eval` function, which can be used to execute
/// arbitrary code.
///
/// ## Example
/// ```python
/// import logging
///
/// logging.config.listen(9999)
/// ```
///
/// ## References
/// - [Python documentation: `logging.config.listen()`](https://docs.python.org/3/library/logging.config.html#logging.config.listen)
#[violation]
pub struct LoggingConfigInsecureListen;

impl Violation for LoggingConfigInsecureListen {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of insecure `logging.config.listen` detected")
    }
}

/// S612
pub(crate) fn logging_config_insecure_listen(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::LOGGING) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["logging", "config", "listen"])
        })
    {
        if call.arguments.find_keyword("verify").is_some() {
            return;
        }

        checker.diagnostics.push(Diagnostic::new(
            LoggingConfigInsecureListen,
            call.func.range(),
        ));
    }
}
