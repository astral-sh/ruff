use ruff_python_ast::call_path::{collect_call_path, from_qualified_name};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_ast::{self as ast, Arguments, Expr, Keyword};

use crate::model::SemanticModel;

/// Return `true` if the given `Expr` is a potential logging call. Matches
/// `logging.error`, `logger.error`, `self.logger.error`, etc., but not
/// arbitrary `foo.error` calls.
///
/// It also matches direct `logging.error` calls when the `logging` module
/// is aliased. Example:
/// ```python
/// import logging as bar
///
/// # This is detected to be a logger candidate.
/// bar.error()
/// ```
pub fn is_logger_candidate(
    func: &Expr,
    semantic: &SemanticModel,
    logger_objects: &[String],
) -> bool {
    let Expr::Attribute(ast::ExprAttribute { value, .. }) = func else {
        return false;
    };

    // If the symbol was imported from another module, ensure that it's either a user-specified
    // logger object, the `logging` module itself, or `flask.current_app.logger`.
    if let Some(call_path) = semantic.resolve_call_path(value) {
        if matches!(
            call_path.as_slice(),
            ["logging"] | ["flask", "current_app", "logger"]
        ) {
            return true;
        }

        if logger_objects
            .iter()
            .any(|logger| from_qualified_name(logger) == call_path)
        {
            return true;
        }

        return false;
    }

    // Otherwise, if the symbol was defined in the current module, match against some common
    // logger names.
    if let Some(call_path) = collect_call_path(value) {
        if let Some(tail) = call_path.last() {
            if tail.starts_with("log")
                || tail.ends_with("logger")
                || tail.ends_with("logging")
                || tail.starts_with("LOG")
                || tail.ends_with("LOGGER")
                || tail.ends_with("LOGGING")
            {
                return true;
            }
        }
    }

    false
}

/// If the keywords to a  logging call contain `exc_info=True` or `exc_info=sys.exc_info()`,
/// return the `Keyword` for `exc_info`.
pub fn exc_info<'a>(arguments: &'a Arguments, semantic: &SemanticModel) -> Option<&'a Keyword> {
    let exc_info = arguments.find_keyword("exc_info")?;

    // Ex) `logging.error("...", exc_info=True)`
    if is_const_true(&exc_info.value) {
        return Some(exc_info);
    }

    // Ex) `logging.error("...", exc_info=sys.exc_info())`
    if exc_info
        .value
        .as_call_expr()
        .and_then(|call| semantic.resolve_call_path(&call.func))
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["sys", "exc_info"]))
    {
        return Some(exc_info);
    }

    None
}
