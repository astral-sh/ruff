use ruff_python_ast::{
    self as ast, Arguments, Expr, ExprKind, InterpolatedStringElementKind, Keyword, Operator,
    StringFlags,
};

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
    msg: Expr<'_>,
    f_string: ast::ExprFString<'_>,
    arguments: Arguments<'_>,
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
    if arguments.args().len() > msg_pos + 1 {
        return;
    }

    let mut format_string = String::new();
    let mut args: Vec<&str> = Vec::new();

    // Try to reuse the first part's quote style when building the replacement.
    // Default to double quotes if we can't determine it.
    let quote_str = f_string
        .value()
        .iter()
        .map(|part| match part {
            ast::FStringPartView::Literal(literal) => literal.flags().quote_str(),
            ast::FStringPartView::FString(f) => f.flags().quote_str(),
        })
        .next()
        .unwrap_or("\"");

    for part in f_string.value() {
        match part {
            ast::FStringPartView::Literal(literal) => {
                let literal_text = literal.as_str();
                if literal_text.contains('%') {
                    return;
                }
                format_string.push_str(literal_text);
            }
            ast::FStringPartView::FString(f) => {
                for element in f.elements() {
                    match element.classify() {
                        InterpolatedStringElementKind::Literal(lit) => {
                            // If the literal text contains a '%' placeholder, bail out: mixing
                            // f-string interpolation with '%' placeholders is ambiguous for our
                            // automatic conversion, so don't offer a fix for this case.
                            if lit.value().contains('%') {
                                return;
                            }
                            format_string.push_str(lit.value().as_ref());
                        }
                        InterpolatedStringElementKind::Interpolation(interpolated) => {
                            if interpolated.format_spec().is_some()
                                || !matches!(
                                    interpolated.conversion(),
                                    ruff_python_ast::ConversionFlag::None
                                )
                            {
                                return;
                            }
                            match interpolated.expression().classify() {
                                ExprKind::Name(name) => {
                                    format_string.push_str("%s");
                                    args.push(name.id().as_str());
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
fn check_msg(checker: &Checker, msg: Expr<'_>, arguments: Arguments<'_>, msg_pos: usize) {
    match msg.classify() {
        // Check for string concatenation and percent format.
        ExprKind::BinOp(expr_bin_op) => {
            let op = expr_bin_op.op();
            match op {
                Operator::Add => {
                    checker.report_diagnostic_if_enabled(LoggingStringConcat, msg.range());
                }
                Operator::Mod => {
                    checker.report_diagnostic_if_enabled(LoggingPercentFormat, msg.range());
                }
                _ => {}
            }
        }
        // Check for f-strings.
        ExprKind::FString(f_string) if checker.is_rule_enabled(Rule::LoggingFString) => {
            logging_f_string(checker, msg, f_string, arguments, msg_pos);
        }
        // Check for .format() calls.
        ExprKind::Call(expr_call) if checker.is_rule_enabled(Rule::LoggingStringFormat) => {
            let func = expr_call.func();

            if let ExprKind::Attribute(attribute_node) = func.classify() {
                let value = attribute_node.value();
                let attr = attribute_node.attr();
                if attr == "format" && value.is_literal_expr() {
                    checker.report_diagnostic(LoggingStringFormat, msg.range());
                }
            }
        }
        _ => {}
    }
}

/// Check contents of the `extra` argument to logging calls.
fn check_log_record_attr_clash(checker: &Checker, extra: Keyword<'_>) {
    match extra.value().classify() {
        ExprKind::Dict(dict) => {
            for invalid_key in dict.iter_keys().filter_map(|key| {
                let string_key = key?.as_string_literal_expr()?;
                if is_reserved_attr(string_key.value().to_str()) {
                    Some(string_key)
                } else {
                    None
                }
            }) {
                checker.report_diagnostic(
                    LoggingExtraAttrClash(invalid_key.value().to_string()),
                    invalid_key.range(),
                );
            }
        }
        ExprKind::Call(expr_call)
            if checker
                .semantic()
                .match_builtin_expr(expr_call.func(), "dict") =>
        {
            let arguments = expr_call.arguments();

            let keywords = arguments.keywords();
            for keyword in keywords {
                if let Some(attr) = keyword.arg() {
                    if is_reserved_attr(attr.as_str()) {
                        checker.report_diagnostic(
                            LoggingExtraAttrClash(attr.to_string()),
                            keyword.range(),
                        );
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
    call: ast::ExprCall<'_>,
) -> Option<(LoggingCallType, TextRange)> {
    // Determine the call type (e.g., `info` vs. `exception`) and the range of the attribute.
    match call.func().classify() {
        ExprKind::Attribute(expr_attribute) => {
            let attr = expr_attribute.attr();

            let call_type = LoggingCallType::from_attribute(attr.as_str())?;
            if !logging::is_logger_candidate(
                call.func(),
                checker.semantic(),
                &checker.settings().logger_objects,
            ) {
                return None;
            }
            Some((call_type, attr.range()))
        }
        ExprKind::Name(_) => {
            let qualified_name = checker.semantic().resolve_qualified_name(call.func())?;
            let ["logging", attribute] = qualified_name.segments() else {
                return None;
            };
            let call_type = LoggingCallType::from_attribute(attribute)?;
            Some((call_type, call.func().range()))
        }
        _ => None,
    }
}

/// Check logging calls for violations.
pub(crate) fn logging_call(checker: &Checker, call: ast::ExprCall<'_>) {
    let Some((logging_call_type, range)) = find_logging_call(checker, call) else {
        return;
    };

    // G001, G002, G003, G004
    let msg_pos = usize::from(matches!(logging_call_type, LoggingCallType::LogCall));
    if let Some(format_arg) = call.arguments().find_argument_value("msg", msg_pos) {
        check_msg(checker, format_arg, call.arguments(), msg_pos);
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
        if let Some(extra) = call.arguments().find_keyword("extra") {
            check_log_record_attr_clash(checker, extra);
        }
    }

    // G201, G202
    if checker.any_rule_enabled(&[Rule::LoggingExcInfo, Rule::LoggingRedundantExcInfo]) {
        if !checker.semantic().in_exception_handler() {
            return;
        }
        let Some(exc_info) = logging::exc_info(call.arguments(), checker.semantic()) else {
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
