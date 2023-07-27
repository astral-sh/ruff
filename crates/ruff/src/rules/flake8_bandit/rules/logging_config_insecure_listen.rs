use ruff_python_ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::find_keyword;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for insecure `logging.config.listen` calls.
///
/// ## Why is this bad?
/// `logging.config.listen` starts a server that listens for logging
/// configuration requests. This is insecure as parts of the configuration are
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
pub(crate) fn logging_config_insecure_listen(
    checker: &mut Checker,
    func: &Expr,
    keywords: &[Keyword],
) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["logging", "config", "listen"])
        })
    {
        if find_keyword(keywords, "verify").is_some() {
            return;
        }

        checker
            .diagnostics
            .push(Diagnostic::new(LoggingConfigInsecureListen, func.range()));
    }
}
