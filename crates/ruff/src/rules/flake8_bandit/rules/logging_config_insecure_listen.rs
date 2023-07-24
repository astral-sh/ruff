use rustpython_parser::ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::find_keyword;

use crate::checkers::ast::Checker;

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
