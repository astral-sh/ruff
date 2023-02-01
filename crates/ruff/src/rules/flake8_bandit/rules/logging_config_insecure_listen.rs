use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, Keyword};

use crate::ast::helpers::SimpleCallArgs;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct LoggingConfigInsecureListen;
);
impl Violation for LoggingConfigInsecureListen {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of insecure `logging.config.listen` detected")
    }
}

/// S612
pub fn logging_config_insecure_listen(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["logging", "config", "listen"]
    }) {
        let call_args = SimpleCallArgs::new(args, keywords);

        if call_args.get_argument("verify", None).is_none() {
            checker.diagnostics.push(Diagnostic::new(
                LoggingConfigInsecureListen,
                Range::from_located(func),
            ));
        }
    }
}
