use rustpython_parser::ast::{Expr, ExprKind};

use ruff_python_ast::call_path::collect_call_path;

use crate::context::Context;

/// Return `true` if the given `Expr` is a potential logging call. Matches
/// `logging.error`, `logger.error`, `self.logger.error`, etc., but not
/// arbitrary `foo.error` calls.
///
/// It even matches direct `logging.error` calls even if the `logging` module
/// is aliased. Example:
/// ```python
/// import logging as bar
///
/// # This is detected to be a logger candidate
/// bar.error()
/// ```
pub fn is_logger_candidate(context: &Context, func: &Expr) -> bool {
    if let ExprKind::Attribute { value, .. } = &func.node {
        let Some(call_path) = (if let Some(call_path) = context.resolve_call_path(value) {
            if call_path.first().map_or(false, |module| *module == "logging") {
                Some(call_path)
            } else {
                None
            }
        } else {
            collect_call_path(value)
        }) else {
            return false;
        };
        if let Some(tail) = call_path.last() {
            if tail.starts_with("log") || tail.ends_with("logger") || tail.ends_with("logging") {
                return true;
            }
        }
    }
    false
}
