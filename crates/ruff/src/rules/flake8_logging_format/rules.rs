use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword, Location, Operator};

use crate::ast::helpers::{find_keyword, is_logger_candidate, SimpleCallArgs};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::rules::flake8_logging_format::violations::{
    LoggingExcInfo, LoggingExtraAttrClash, LoggingFString, LoggingPercentFormat,
    LoggingRedundantExcInfo, LoggingStringConcat, LoggingStringFormat, LoggingWarn,
};

enum LoggingLevel {
    Debug,
    Critical,
    Error,
    Exception,
    Info,
    Warn,
    Warning,
}

impl LoggingLevel {
    fn from_str(level: &str) -> Option<Self> {
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

const RESERVED_ATTRS: &[&str; 22] = &[
    "args",
    "asctime",
    "created",
    "exc_info",
    "exc_text",
    "filename",
    "funcName",
    "levelname",
    "levelno",
    "lineno",
    "module",
    "msecs",
    "message",
    "msg",
    "name",
    "pathname",
    "process",
    "processName",
    "relativeCreated",
    "stack_info",
    "thread",
    "threadName",
];

/// Check logging messages for violations.
fn check_msg(checker: &mut Checker, msg: &Expr) {
    match &msg.node {
        // Check for string concatenation and percent format.
        ExprKind::BinOp { op, .. } => match op {
            Operator::Add => {
                if checker.settings.rules.enabled(&Rule::LoggingStringConcat) {
                    checker.diagnostics.push(Diagnostic::new(
                        LoggingStringConcat,
                        Range::from_located(msg),
                    ));
                }
            }
            Operator::Mod => {
                if checker.settings.rules.enabled(&Rule::LoggingPercentFormat) {
                    checker.diagnostics.push(Diagnostic::new(
                        LoggingPercentFormat,
                        Range::from_located(msg),
                    ));
                }
            }
            _ => {}
        },
        // Check for f-strings.
        ExprKind::JoinedStr { .. } => {
            if checker.settings.rules.enabled(&Rule::LoggingFString) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(LoggingFString, Range::from_located(msg)));
            }
        }
        // Check for .format() calls.
        ExprKind::Call { func, .. } => {
            if checker.settings.rules.enabled(&Rule::LoggingStringFormat) {
                if let ExprKind::Attribute { value, attr, .. } = &func.node {
                    if attr == "format" && matches!(value.node, ExprKind::Constant { .. }) {
                        checker.diagnostics.push(Diagnostic::new(
                            LoggingStringFormat,
                            Range::from_located(msg),
                        ));
                    }
                }
            }
        }
        _ => {}
    }
}

/// Check contents of the `extra` argument to logging calls.
fn check_log_record_attr_clash(checker: &mut Checker, extra: &Keyword) {
    match &extra.node.value.node {
        ExprKind::Dict { keys, .. } => {
            for key in keys {
                if let Some(key) = &key {
                    if let ExprKind::Constant {
                        value: Constant::Str(string),
                        ..
                    } = &key.node
                    {
                        if RESERVED_ATTRS.contains(&string.as_str()) {
                            checker.diagnostics.push(Diagnostic::new(
                                LoggingExtraAttrClash(string.to_string()),
                                Range::from_located(key),
                            ));
                        }
                    }
                }
            }
        }
        ExprKind::Call { func, keywords, .. } => {
            if checker
                .resolve_call_path(func)
                .map_or(false, |call_path| call_path.as_slice() == ["", "dict"])
            {
                for keyword in keywords {
                    if let Some(key) = &keyword.node.arg {
                        if RESERVED_ATTRS.contains(&key.as_str()) {
                            checker.diagnostics.push(Diagnostic::new(
                                LoggingExtraAttrClash(key.to_string()),
                                Range::from_located(keyword),
                            ));
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Check logging calls for violations.
pub fn logging_call(checker: &mut Checker, func: &Expr, args: &[Expr], keywords: &[Keyword]) {
    if !is_logger_candidate(func) {
        return;
    }

    if let ExprKind::Attribute { value, attr, .. } = &func.node {
        if let Some(logging_level) = LoggingLevel::from_str(attr.as_str()) {
            let call_args = SimpleCallArgs::new(args, keywords);
            let level_call_range = Range::new(
                Location::new(
                    func.location.row(),
                    value.end_location.unwrap().column() + 1,
                ),
                Location::new(
                    func.end_location.unwrap().row(),
                    func.end_location.unwrap().column(),
                ),
            );

            // G001 - G004
            if let Some(format_arg) = call_args.get_argument("msg", Some(0)) {
                check_msg(checker, format_arg);
            }

            // G010
            if checker.settings.rules.enabled(&Rule::LoggingWarn)
                && matches!(logging_level, LoggingLevel::Warn)
            {
                let mut diagnostic = Diagnostic::new(LoggingWarn, level_call_range);
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.amend(Fix::replacement(
                        "warning".to_string(),
                        level_call_range.location,
                        level_call_range.end_location,
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }

            // G101
            if checker.settings.rules.enabled(&Rule::LoggingExtraAttrClash) {
                if let Some(extra) = find_keyword(keywords, "extra") {
                    check_log_record_attr_clash(checker, extra);
                }
            }

            // G201, G202
            if checker.settings.rules.enabled(&Rule::LoggingExcInfo)
                || checker
                    .settings
                    .rules
                    .enabled(&Rule::LoggingRedundantExcInfo)
            {
                if !checker.in_exception_handler() {
                    return;
                }
                if let Some(exc_info) = find_keyword(keywords, "exc_info") {
                    // If `exc_info` is `True` or `sys.exc_info()`, it's redundant; but otherwise,
                    // return.
                    if !(matches!(
                        exc_info.node.value.node,
                        ExprKind::Constant {
                            value: Constant::Bool(true),
                            ..
                        }
                    ) || if let ExprKind::Call { func, .. } = &exc_info.node.value.node {
                        checker.resolve_call_path(func).map_or(false, |call_path| {
                            call_path.as_slice() == ["sys", "exc_info"]
                        })
                    } else {
                        false
                    }) {
                        return;
                    }

                    match logging_level {
                        LoggingLevel::Error => {
                            if checker.settings.rules.enabled(&Rule::LoggingExcInfo) {
                                checker
                                    .diagnostics
                                    .push(Diagnostic::new(LoggingExcInfo, level_call_range));
                            }
                        }
                        LoggingLevel::Exception => {
                            if checker
                                .settings
                                .rules
                                .enabled(&Rule::LoggingRedundantExcInfo)
                            {
                                checker.diagnostics.push(Diagnostic::new(
                                    LoggingRedundantExcInfo,
                                    Range::from_located(exc_info),
                                ));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
