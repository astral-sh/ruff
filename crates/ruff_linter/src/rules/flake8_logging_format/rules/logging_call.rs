use ruff_python_ast::{
    self as ast, Arguments, ConversionFlag, Expr, InterpolatedStringElement, Keyword, Operator,
};
use ruff_python_codegen::Generator;
use ruff_python_semantic::analyze::logging;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::flake8_logging_format::violations::{
    LoggingExcInfo, LoggingExtraAttrClash, LoggingFString, LoggingPercentFormat,
    LoggingRedundantExcInfo, LoggingStringConcat, LoggingStringFormat, LoggingWarn,
};
use crate::{Edit, Fix};

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
                checker.report_diagnostic_if_enabled(LoggingStringConcat, msg.range());
            }
            Operator::Mod => {
                checker.report_diagnostic_if_enabled(LoggingPercentFormat, msg.range());
            }
            _ => {}
        },
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

/// Convert an f-string to %-style formatting for logging calls.
fn convert_f_string_to_percent_format(
    f_string: &ast::ExprFString,
    call: &ast::ExprCall,
    msg_pos: usize,
    generator: Generator,
) -> ruff_diagnostics::Fix {
    let mut format_string = String::new();
    let mut arguments = Vec::new();

    // Process all f-string elements
    for element in f_string.value.elements() {
        match element {
            InterpolatedStringElement::Literal(literal) => {
                format_string.push_str(&literal.value);
            }
            InterpolatedStringElement::Interpolation(interpolation) => {
                // Convert the interpolation to a % format specifier
                let format_spec = convert_interpolation_to_percent_format(interpolation);
                format_string.push_str(&format_spec);
                arguments.push(interpolation.expression.as_ref().clone());
            }
        }
    }

    // Create the new format string literal
    let format_string_literal = ast::ExprStringLiteral {
        value: ast::StringLiteralValue::single(ast::StringLiteral {
            value: format_string.into_boxed_str(),
            flags: ast::StringLiteralFlags::empty(),
            range: f_string.range(),
            node_index: ast::AtomicNodeIndex::dummy(),
        }),
        range: f_string.range(),
        node_index: ast::AtomicNodeIndex::dummy(),
    };

    // Build a new arguments list for the logging call.
    // Preserve existing arguments before the message position.
    let mut new_args: Vec<ast::Expr> = call.arguments.args[..msg_pos].to_vec();

    // Add the format string at the message position.
    new_args.push(format_string_literal.into());

    // Add the extracted f-string arguments.
    new_args.extend(arguments);

    // Add any remaining arguments after the original message position.
    if msg_pos + 1 < call.arguments.args.len() {
        new_args.extend_from_slice(&call.arguments.args[msg_pos + 1..]);
    }

    // Create new call arguments.
    let new_arguments = ast::Arguments {
        args: new_args.into(),
        keywords: call.arguments.keywords.clone(),
        range: call.arguments.range,
        node_index: ast::AtomicNodeIndex::dummy(),
    };

    // Create the new call.
    let new_call = ast::ExprCall {
        func: call.func.clone(),
        arguments: new_arguments,
        range: call.range,
        node_index: ast::AtomicNodeIndex::dummy(),
    };

    let replacement = generator.expr(&new_call.into());

    Fix::safe_edit(Edit::range_replacement(replacement, call.range))
}

/// Convert an f-string interpolation to a % format specifier.
fn convert_interpolation_to_percent_format(
    interpolation: &ast::InterpolatedElement,
) -> std::string::String {
    let mut format_spec = String::from("%");

    // Handle conversion flags
    match interpolation.conversion {
        ConversionFlag::Str => format_spec.push('s'),
        ConversionFlag::Repr => format_spec.push('r'),
        ConversionFlag::Ascii => format_spec.push('a'),
        ConversionFlag::None => {
            // Check if there's a format spec to determine the type
            if let Some(format_spec_node) = &interpolation.format_spec {
                let spec_text = extract_format_spec_text(format_spec_node);
                if spec_text.is_empty() {
                    format_spec.push('s');
                } else {
                    format_spec.push_str(&convert_format_spec_to_percent(&spec_text));
                }
            } else {
                format_spec.push('s');
            }
        }
    }

    format_spec
}

/// Extract the format specification text from a format spec node.
fn extract_format_spec_text(
    format_spec: &ast::InterpolatedStringFormatSpec,
) -> std::string::String {
    let mut spec_text = String::new();

    for element in &format_spec.elements {
        match element {
            InterpolatedStringElement::Literal(literal) => {
                spec_text.push_str(&literal.value);
            }
            InterpolatedStringElement::Interpolation(_) => {
                // Nested interpolations in format specs are complex, fall back to default
                return String::new();
            }
        }
    }

    spec_text
}

/// Convert f-string format specification to % format specification.
fn convert_format_spec_to_percent(specifier: &str) -> std::string::String {
    // Handle common format specifications
    match specifier {
        // Float formatting
        s if s.ends_with('e') => s.to_string(),
        s if s.ends_with('f') => s.to_string(),
        s if s.ends_with('g') => s.to_string(),

        // Integer formatting
        s if s.ends_with('d') => s.to_string(),
        s if s.ends_with('o') => s.to_string(),
        s if s.ends_with('x') => s.to_string(),
        s if s.ends_with('X') => s.to_string(),

        // String formatting
        s if s.ends_with('s') => s.to_string(),

        // Default to string for other cases
        _ => "s".to_string(),
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
                &checker.settings().logger_objects,
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

    // G001, G002, G003, G004
    let msg_pos = usize::from(matches!(logging_call_type, LoggingCallType::LogCall));
    if let Some(format_arg) = call.arguments.find_argument_value("msg", msg_pos) {
        // Check for f-strings (G004) - handle this in a specific way to access the full call
        if let Expr::FString(f_string) = format_arg {
            if checker.is_rule_enabled(Rule::LoggingFString) {
                let mut diagnostic = checker.report_diagnostic(LoggingFString, format_arg.range());
                diagnostic.try_set_fix(|| {
                    Ok(convert_f_string_to_percent_format(
                        f_string,
                        call,
                        msg_pos,
                        checker.generator(),
                    ))
                });
            }
        } else {
            // Check other format violations (G001, G002, G003)
            check_msg(checker, format_arg);
        }
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
