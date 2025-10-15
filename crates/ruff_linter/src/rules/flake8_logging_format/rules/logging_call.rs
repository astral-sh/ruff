use ruff_python_ast::InterpolatedStringElement;
use ruff_python_ast::{self as ast, Arguments, Expr, Keyword, Operator, StringFlags};

use ruff_python_semantic::analyze::logging;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::preview::is_fix_f_string_logging_enabled;
use crate::registry::Rule;
use crate::rules::flake8_logging_format::violations::{
    LoggingExcInfo, LoggingExtraAttrClash, LoggingFString, LoggingPercentFormat,
    LoggingRedundantExcInfo, LoggingStringConcat, LoggingStringFormat, LoggingWarn,
};
use crate::{Edit, Fix};

fn logging_f_string(
    checker: &Checker,
    msg: &Expr,
    f_string: &ast::ExprFString,
    arguments: &Arguments,
    msg_pos: usize,
) {
    // Report the diagnostic up-front so we can attach a fix later only when preview is enabled.
    let mut diagnostic = checker.report_diagnostic(LoggingFString, msg.range());

    // Preview gate for the automatic fix.
    if !is_fix_f_string_logging_enabled(checker.settings()) {
        return;
    }

    // If there are existing positional arguments after the message, bail out.
    // This could indicate a mistake or complex usage we shouldn't try to fix.
    if arguments.args.len() > msg_pos + 1 {
        return;
    }

    let mut format_string = String::new();
    let mut args: Vec<&str> = Vec::new();

    // Try to reuse the first part's quote style when building the replacement.
    // Default to double quotes if we can't determine it.
    let quote_str = f_string
        .value
        .iter()
        .map(|part| match part {
            ast::FStringPart::Literal(literal) => literal.flags.quote_str(),
            ast::FStringPart::FString(f) => f.flags.quote_str(),
        })
        .next()
        .unwrap_or("\"");

    for part in &f_string.value {
        match part {
            ast::FStringPart::Literal(literal) => {
                let literal_text = literal.as_str();
                if literal_text.contains('%') {
                    return;
                }
                format_string.push_str(literal_text);
            }
            ast::FStringPart::FString(f) => {
                for element in &f.elements {
                    match element {
                        InterpolatedStringElement::Literal(lit) => {
                            // If the literal text contains a '%' placeholder, bail out: mixing
                            // f-string interpolation with '%' placeholders is ambiguous for our
                            // automatic conversion, so don't offer a fix for this case.
                            if lit.value.as_ref().contains('%') {
                                return;
                            }
                            format_string.push_str(lit.value.as_ref());
                        }
                        InterpolatedStringElement::Interpolation(interpolated) => {
                            if interpolated.format_spec.is_some()
                                || !matches!(
                                    interpolated.conversion,
                                    ruff_python_ast::ConversionFlag::None
                                )
                            {
                                return;
                            }
                            match interpolated.expression.as_ref() {
                                Expr::Name(name) => {
                                    format_string.push_str("%s");
                                    args.push(name.id.as_str());
                                }
                                _ => return,
                            }
                        }
                    }
                }
            }
        }
    }

    if args.is_empty() {
        return;
    }

    let replacement = format!(
        "{q}{format_string}{q}, {args}",
        q = quote_str,
        format_string = format_string,
        args = args.join(", ")
    );

    let fix = Fix::safe_edit(Edit::range_replacement(replacement, msg.range()));
    diagnostic.set_fix(fix);
}

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
fn check_msg(checker: &Checker, msg: &Expr, arguments: &Arguments, msg_pos: usize) {
    match msg {
        // Check for string concatenation and percent format.
        Expr::BinOp(ast::ExprBinOp { op, .. }) => match op {
            Operator::Add => {
                checker.report_diagnostic_if_enabled(LoggingStringConcat, msg.range());
            }
            Operator::Mod => {
                checker.report_diagnostic_if_enabled(LoggingPercentFormat, msg.range());
            }
            _ => {}
        },
        // Check for f-strings.
        Expr::FString(f_string) => {
            if checker.is_rule_enabled(Rule::LoggingFString) {
                logging_f_string(checker, msg, f_string, arguments, msg_pos);
            }
        }
        // Check for .format() calls.
        Expr::Call(ast::ExprCall { func, .. }) => {
            if checker.is_rule_enabled(Rule::LoggingStringFormat) {
                if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() {
                    if attr == "format" && value.is_literal_expr() {
                        checker.report_diagnostic(LoggingStringFormat, msg.range());
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
                checker.report_diagnostic(
                    LoggingExtraAttrClash(invalid_key.value.to_string()),
                    invalid_key.range(),
                );
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
                            checker.report_diagnostic(
                                LoggingExtraAttrClash(attr.to_string()),
                                keyword.range(),
                            );
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum LoggingCallType {
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

pub(crate) fn find_logging_call(
    checker: &Checker,
    call: &ast::ExprCall,
) -> Option<(LoggingCallType, TextRange)> {
    // Determine the call type (e.g., `info` vs. `exception`) and the range of the attribute.
    match call.func.as_ref() {
        Expr::Attribute(ast::ExprAttribute { value: _, attr, .. }) => {
            let call_type = LoggingCallType::from_attribute(attr.as_str())?;
            if !logging::is_logger_candidate(
                &call.func,
                checker.semantic(),
                &checker.settings().logger_objects,
            ) {
                return None;
            }
            Some((call_type, attr.range()))
        }
        Expr::Name(_) => {
            let qualified_name = checker
                .semantic()
                .resolve_qualified_name(call.func.as_ref())?;
            let ["logging", attribute] = qualified_name.segments() else {
                return None;
            };
            let call_type = LoggingCallType::from_attribute(attribute)?;
            Some((call_type, call.func.range()))
        }
        _ => None,
    }
}

/// Check logging calls for violations.
pub(crate) fn logging_call(checker: &Checker, call: &ast::ExprCall) {
    let Some((logging_call_type, range)) = find_logging_call(checker, call) else {
        return;
    };

    // G001, G002, G003, G004
    let msg_pos = usize::from(matches!(logging_call_type, LoggingCallType::LogCall));
    if let Some(format_arg) = call.arguments.find_argument_value("msg", msg_pos) {
        check_msg(checker, format_arg, &call.arguments, msg_pos);
    }

    // G010
    if checker.is_rule_enabled(Rule::LoggingWarn) {
        if matches!(
            logging_call_type,
            LoggingCallType::LevelCall(LoggingLevel::Warn)
        ) {
            let mut diagnostic = checker.report_diagnostic(LoggingWarn, range);
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                "warning".to_string(),
                range,
            )));
        }
    }

    // G101
    if checker.is_rule_enabled(Rule::LoggingExtraAttrClash) {
        if let Some(extra) = call.arguments.find_keyword("extra") {
            check_log_record_attr_clash(checker, extra);
        }
    }

    // G201, G202
    if checker.any_rule_enabled(&[Rule::LoggingExcInfo, Rule::LoggingRedundantExcInfo]) {
        if !checker.semantic().in_exception_handler() {
            return;
        }
        let Some(exc_info) = logging::exc_info(&call.arguments, checker.semantic()) else {
            return;
        };
        if let LoggingCallType::LevelCall(logging_level) = logging_call_type {
            match logging_level {
                LoggingLevel::Error => {
                    checker.report_diagnostic_if_enabled(LoggingExcInfo, range);
                }
                LoggingLevel::Exception => {
                    checker.report_diagnostic_if_enabled(LoggingRedundantExcInfo, exc_info.range());
                }
                _ => {}
            }
        }
    }
}
