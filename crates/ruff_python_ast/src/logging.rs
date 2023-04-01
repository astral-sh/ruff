use crate::call_path::CallPath;
use rustpython_parser::ast::{Expr, ExprKind};

use crate::context::Context;

#[derive(Copy, Clone)]
pub enum LoggingLevel {
    Debug,
    Critical,
    Error,
    Exception,
    Info,
    Warn,
    Warning,
}

impl LoggingLevel {
    pub fn from_attribute(level: &str) -> Option<Self> {
        match level {
            "debug" => Some(LoggingLevel::Debug),
            "critical" => Some(LoggingLevel::Critical),
            "error" => Some(LoggingLevel::Error),
            "exception" => Some(LoggingLevel::Exception),
            "info" => Some(LoggingLevel::Info),
            "warn" => Some(LoggingLevel::Warn),
            "warning" => Some(LoggingLevel::Warning),
            _ => None,
        }
    }
}

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
        if let Some(call_path) = context
            .resolve_call_path(value)
            .or_else(|| CallPath::try_from_expr(value))
        {
            let tail = call_path.last();
            if let Some(tail) = call_path.last() {
                if tail.starts_with("log") || tail.ends_with("logger") || tail.ends_with("logging")
                {
                    return true;
                }
            }
        }
    }
    false
}
