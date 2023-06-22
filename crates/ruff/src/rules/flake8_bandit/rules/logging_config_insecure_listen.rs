use rustpython_parser::ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::SimpleCallArgs;

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
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["logging", "config", "listen"])
        })
    {
        let call_args = SimpleCallArgs::new(args, keywords);

        if call_args.keyword_argument("verify").is_none() {
            checker
                .diagnostics
                .push(Diagnostic::new(LoggingConfigInsecureListen, func.range()));
        }
    }
}
