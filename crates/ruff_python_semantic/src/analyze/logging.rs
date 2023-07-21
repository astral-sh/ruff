use rustpython_parser::ast::{self, Constant, Expr, Keyword};

use ruff_python_ast::call_path::collect_call_path;
use ruff_python_ast::helpers::find_keyword;

use crate::model::SemanticModel;

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
pub fn is_logger_candidate(func: &Expr, semantic: &SemanticModel) -> bool {
    if let Expr::Attribute(ast::ExprAttribute { value, .. }) = func {
        let Some(call_path) = (if let Some(call_path) = semantic.resolve_call_path(value) {
            if call_path
                .first()
                .map_or(false, |module| *module == "logging")
                || call_path.as_slice() == ["flask", "current_app", "logger"]
            {
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

/// If the keywords to a  logging call contain `exc_info=True` or `exc_info=sys.exc_info()`,
/// return the `Keyword` for `exc_info`.
pub fn exc_info<'a>(keywords: &'a [Keyword], semantic: &SemanticModel) -> Option<&'a Keyword> {
    let exc_info = find_keyword(keywords, "exc_info")?;

    // Ex) `logging.error("...", exc_info=True)`
    if matches!(
        exc_info.value,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Bool(true),
            ..
        })
    ) {
        return Some(exc_info);
    }

    // Ex) `logging.error("...", exc_info=sys.exc_info())`
    if let Expr::Call(ast::ExprCall { func, .. }) = &exc_info.value {
        if semantic.resolve_call_path(func).map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["sys", "exc_info"])
        }) {
            return Some(exc_info);
        }
    }

    None
}
