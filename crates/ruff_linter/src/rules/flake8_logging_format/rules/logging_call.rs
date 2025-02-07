use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_python_ast::{self as ast, Arguments, Expr, Keyword, Operator};
use ruff_python_semantic::analyze::logging;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::flake8_logging_format::violations::{
    LoggingExcInfo, LoggingExtraAttrClash, LoggingFString, LoggingPercentFormat,
    LoggingRedundantExcInfo, LoggingStringConcat, LoggingStringFormat, LoggingWarn,
};

/// Returns `true` if the attribute is a reserved attribute on the `logging` module's `LogRecord`
/// class.
fn is_reserved_attr(attr: &str) -> bool {
    matches!(
        attr,
        "args"
            | "asctime"
            | "created"
            | "exc_info"
            | "exc_text"
            | "filename"
            | "funcName"
            | "levelname"
            | "levelno"
            | "lineno"
            | "module"
            | "msecs"
            | "message"
            | "msg"
            | "name"
            | "pathname"
            | "process"
            | "processName"
            | "relativeCreated"
            | "stack_info"
            | "thread"
            | "threadName"
    )
}

/// Check logging messages for violations.
fn check_msg(checker: &Checker, msg: &Expr) {
    match msg {
        // Check for string concatenation and percent format.
        Expr::BinOp(ast::ExprBinOp { op, .. }) => match op {
            Operator::Add => {
                if checker.enabled(Rule::LoggingStringConcat) {
                    checker.report_diagnostic(Diagnostic::new(LoggingStringConcat, msg.range()));
                }
            }
            Operator::Mod => {
                if checker.enabled(Rule::LoggingPercentFormat) {
                    checker.report_diagnostic(Diagnostic::new(LoggingPercentFormat, msg.range()));
                }
            }
            _ => {}
        },
        // Check for f-strings.
        Expr::FString(_) => {
            if checker.enabled(Rule::LoggingFString) {
                checker.report_diagnostic(Diagnostic::new(LoggingFString, msg.range()));
            }
        }
        // Check for .format() calls.
        Expr::Call(ast::ExprCall { func, .. }) => {
            if checker.enabled(Rule::LoggingStringFormat) {
                if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() {
                    if attr == "format" && value.is_literal_expr() {
                        checker
                            .report_diagnostic(Diagnostic::new(LoggingStringFormat, msg.range()));
                    }
                }
            }
        }
        _ => {}
    }
}

/// Check contents of the `extra` argument to logging calls.
fn check_log_record_attr_clash(checker: &Checker, extra: &Keyword) {
    match &extra.value {
        Expr::Dict(dict) => {
            for invalid_key in dict.iter_keys().filter_map(|key| {
                let string_key = key?.as_string_literal_expr()?;
                if is_reserved_attr(string_key.value.to_str()) {
                    Some(string_key)
                } else {
                    None
                }
            }) {
                checker.report_diagnostic(Diagnostic::new(
                    LoggingExtraAttrClash(invalid_key.value.to_string()),
                    invalid_key.range(),
                ));
            }
        }
        Expr::Call(ast::ExprCall {
            func,
            arguments: Arguments { keywords, .. },
            ..
        }) => {
            if checker.semantic().match_builtin_expr(func, "dict") {
                for keyword in keywords {
                    if let Some(attr) = &keyword.arg {
                        if is_reserved_attr(attr) {
                            checker.report_diagnostic(Diagnostic::new(
                                LoggingExtraAttrClash(attr.to_string()),
                                keyword.range(),
                            ));
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

#[derive(Debug, Copy, Clone)]
enum LoggingCallType {
    /// Logging call with a level method, e.g., `logging.info`.
    LevelCall(LoggingLevel),
    /// Logging call with an integer level as an argument, e.g., `logger.log(level, ...)`.
    LogCall,
}

impl LoggingCallType {
    fn from_attribute(attr: &str) -> Option<Self> {
        if attr == "log" {
            Some(LoggingCallType::LogCall)
        } else {
            LoggingLevel::from_attribute(attr).map(LoggingCallType::LevelCall)
        }
    }
}

/// Check logging calls for violations.
pub(crate) fn logging_call(checker: &Checker, call: &ast::ExprCall) {
    // Determine the call type (e.g., `info` vs. `exception`) and the range of the attribute.
    let (logging_call_type, range) = match call.func.as_ref() {
        Expr::Attribute(ast::ExprAttribute { value: _, attr, .. }) => {
            let Some(call_type) = LoggingCallType::from_attribute(attr.as_str()) else {
                return;
            };
            if !logging::is_logger_candidate(
                &call.func,
                checker.semantic(),
                &checker.settings.logger_objects,
            ) {
                return;
            }
            (call_type, attr.range())
        }
        Expr::Name(_) => {
            let Some(qualified_name) = checker
                .semantic()
                .resolve_qualified_name(call.func.as_ref())
            else {
                return;
            };
            let ["logging", attribute] = qualified_name.segments() else {
                return;
            };
            let Some(call_type) = LoggingCallType::from_attribute(attribute) else {
                return;
            };
            (call_type, call.func.range())
        }
        _ => return,
    };

    // G001 - G004
    let msg_pos = usize::from(matches!(logging_call_type, LoggingCallType::LogCall));
    if let Some(format_arg) = call.arguments.find_argument_value("msg", msg_pos) {
        check_msg(checker, format_arg);
    }

    // G010
    if checker.enabled(Rule::LoggingWarn) {
        if matches!(
            logging_call_type,
            LoggingCallType::LevelCall(LoggingLevel::Warn)
        ) {
            let mut diagnostic = Diagnostic::new(LoggingWarn, range);
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                "warning".to_string(),
                range,
            )));
            checker.report_diagnostic(diagnostic);
        }
    }

    // G101
    if checker.enabled(Rule::LoggingExtraAttrClash) {
        if let Some(extra) = call.arguments.find_keyword("extra") {
            check_log_record_attr_clash(checker, extra);
        }
    }

    // G201, G202
    if checker.any_enabled(&[Rule::LoggingExcInfo, Rule::LoggingRedundantExcInfo]) {
        if !checker.semantic().in_exception_handler() {
            return;
        }
        let Some(exc_info) = logging::exc_info(&call.arguments, checker.semantic()) else {
            return;
        };
        if let LoggingCallType::LevelCall(logging_level) = logging_call_type {
            match logging_level {
                LoggingLevel::Error => {
                    if checker.enabled(Rule::LoggingExcInfo) {
                        checker.report_diagnostic(Diagnostic::new(LoggingExcInfo, range));
                    }
                }
                LoggingLevel::Exception => {
                    if checker.enabled(Rule::LoggingRedundantExcInfo) {
                        checker.report_diagnostic(Diagnostic::new(
                            LoggingRedundantExcInfo,
                            exc_info.range(),
                        ));
                    }
                }
                _ => {}
            }
        }
    }
}
