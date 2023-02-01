use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// PGH002 - deprecated use of logging.warn
pub fn deprecated_log_warn(checker: &mut Checker, func: &Expr) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["logging", "warn"]
    }) {
        checker.diagnostics.push(Diagnostic::new(
            violations::DeprecatedLogWarn,
            Range::from_located(func),
        ));
    }
}
