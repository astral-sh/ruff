use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct DeprecatedLogWarn;
);
impl Violation for DeprecatedLogWarn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`warn` is deprecated in favor of `warning`")
    }
}

/// PGH002 - deprecated use of logging.warn
pub fn deprecated_log_warn(checker: &mut Checker, func: &Expr) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["logging", "warn"]
    }) {
        checker.diagnostics.push(Diagnostic::new(
            DeprecatedLogWarn,
            Range::from_located(func),
        ));
    }
}
