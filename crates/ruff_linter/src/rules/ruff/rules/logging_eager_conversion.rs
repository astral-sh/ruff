use std::str::FromStr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_literal::cformat::{
    CConversionFlags, CFormatPart, CFormatSpec, CFormatString, CFormatType,
};
use ruff_python_literal::format::FormatConversion;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::flake8_logging_format::rules::{LoggingCallType, find_logging_call};

/// ## What it does
/// Checks for eager string conversion of arguments to `logging` calls.
///
/// ## Why is this bad?
/// Arguments to `logging` calls will be formatted as strings automatically, so it
/// is unnecessary and less efficient to eagerly format the arguments before passing
/// them in.
///
/// ## Known problems
///
/// This rule detects uses of the `logging` module via a heuristic.
/// Specifically, it matches against:
///
/// - Uses of the `logging` module itself (e.g., `import logging; logging.info(...)`).
/// - Uses of `flask.current_app.logger` (e.g., `from flask import current_app; current_app.logger.info(...)`).
/// - Objects whose name starts with `log` or ends with `logger` or `logging`,
///   when used in the same file in which they are defined (e.g., `logger = logging.getLogger(); logger.info(...)`).
/// - Imported objects marked as loggers via the [`lint.logger-objects`] setting, which can be
///   used to enforce these rules against shared logger objects (e.g., `from module import logger; logger.info(...)`,
///   when [`lint.logger-objects`] is set to `["module.logger"]`).
///
/// ## Example
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(message)s", level=logging.INFO)
///
/// user = "Maria"
///
/// logging.info("%s - Something happened", str(user))
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(message)s", level=logging.INFO)
///
/// user = "Maria"
///
/// logging.info("%s - Something happened", user)
/// ```
///
/// ## Options
/// - `lint.logger-objects`
///
/// ## References
/// - [Python documentation: `logging`](https://docs.python.org/3/library/logging.html)
/// - [Python documentation: Optimization](https://docs.python.org/3/howto/logging.html#optimization)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.13.2")]
pub(crate) struct LoggingEagerConversion {
    pub(crate) format_conversion: FormatConversion,
    pub(crate) function_name: Option<&'static str>,
}

impl Violation for LoggingEagerConversion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoggingEagerConversion {
            format_conversion,
            function_name,
        } = self;
        match (format_conversion, function_name.as_deref()) {
            (FormatConversion::Str, Some("oct")) => {
                "Unnecessary `oct()` conversion when formatting with `%s`. \
            Use `%#o` instead of `%s`"
                    .to_string()
            }
            (FormatConversion::Str, Some("hex")) => {
                "Unnecessary `hex()` conversion when formatting with `%s`. \
            Use `%#x` instead of `%s`"
                    .to_string()
            }
            (FormatConversion::Str, _) => {
                "Unnecessary `str()` conversion when formatting with `%s`".to_string()
            }
            (FormatConversion::Repr, _) => {
                "Unnecessary `repr()` conversion when formatting with `%s`. \
            Use `%r` instead of `%s`"
                    .to_string()
            }
            (FormatConversion::Ascii, _) => {
                "Unnecessary `ascii()` conversion when formatting with `%s`. \
            Use `%a` instead of `%s`"
                    .to_string()
            }
            (FormatConversion::Bytes, _) => {
                "Unnecessary `bytes()` conversion when formatting with `%b`".to_string()
            }
        }
    }
}

/// RUF065
pub(crate) fn logging_eager_conversion(checker: &Checker, call: &ast::ExprCall) {
    let Some((logging_call_type, _range)) = find_logging_call(checker, call) else {
        return;
    };

    let msg_pos = match logging_call_type {
        LoggingCallType::LevelCall(_) => 0,
        LoggingCallType::LogCall => 1,
    };

    // Extract a format string from the logging statement msg argument
    let Some(Expr::StringLiteral(string_literal)) =
        call.arguments.find_argument_value("msg", msg_pos)
    else {
        return;
    };
    let Ok(format_string) = CFormatString::from_str(string_literal.value.to_str()) else {
        return;
    };

    // Iterate over % placeholders in format string and zip with logging statement arguments
    for (spec, arg) in format_string
        .iter()
        .filter_map(|(_, part)| {
            if let CFormatPart::Spec(spec) = part {
                Some(spec)
            } else {
                None
            }
        })
        .zip(call.arguments.args.iter().skip(msg_pos + 1))
    {
        // Check if the argument is a call to eagerly format a value
        if let Expr::Call(ast::ExprCall {
            func,
            arguments: str_call_args,
            ..
        }) = arg
        {
            let CFormatType::String(format_conversion) = spec.format_type else {
                continue;
            };

            // Check for various eager conversion patterns
            match format_conversion {
                // %s with str() - remove str() call
                // Only flag if str() has exactly one argument (positional or keyword) that is not unpacked
                FormatConversion::Str
                    if checker.semantic().match_builtin_expr(func.as_ref(), "str")
                        && str_call_args.len() == 1
                        && str_call_args
                            .find_argument("object", 0)
                            .is_some_and(|arg| !arg.is_variadic()) =>
                {
                    checker.report_diagnostic(
                        LoggingEagerConversion {
                            format_conversion,
                            function_name: None,
                        },
                        arg.range(),
                    );
                }
                // %s with repr() - suggest using %r instead
                FormatConversion::Str
                    if checker.semantic().match_builtin_expr(func.as_ref(), "repr") =>
                {
                    checker.report_diagnostic(
                        LoggingEagerConversion {
                            format_conversion: FormatConversion::Repr,
                            function_name: None,
                        },
                        arg.range(),
                    );
                }
                // %s with ascii() - suggest using %a instead
                FormatConversion::Str
                    if checker
                        .semantic()
                        .match_builtin_expr(func.as_ref(), "ascii") =>
                {
                    checker.report_diagnostic(
                        LoggingEagerConversion {
                            format_conversion: FormatConversion::Ascii,
                            function_name: None,
                        },
                        arg.range(),
                    );
                }
                // %s with oct() - suggest using %#o instead
                FormatConversion::Str
                    if checker.semantic().match_builtin_expr(func.as_ref(), "oct")
                        && !has_complex_conversion_specifier(spec) =>
                {
                    checker.report_diagnostic(
                        LoggingEagerConversion {
                            format_conversion: FormatConversion::Str,
                            function_name: Some("oct"),
                        },
                        arg.range(),
                    );
                }
                // %s with hex() - suggest using %#x instead
                FormatConversion::Str
                    if checker.semantic().match_builtin_expr(func.as_ref(), "hex")
                        && !has_complex_conversion_specifier(spec) =>
                {
                    checker.report_diagnostic(
                        LoggingEagerConversion {
                            format_conversion: FormatConversion::Str,
                            function_name: Some("hex"),
                        },
                        arg.range(),
                    );
                }
                _ => {}
            }
        }
    }
}

/// Check if a conversion specifier has complex flags or precision that make `oct()` or `hex()` necessary.
///
/// Returns `true` if any of these conditions are met:
/// - Flag `0` (zero-pad) is used, flag `-` (left-adjust) is not used, and minimum width is specified
/// - Flag ` ` (blank sign) is used
/// - Flag `+` (sign char) is used
/// - Precision is specified
fn has_complex_conversion_specifier(spec: &CFormatSpec) -> bool {
    if spec.flags.intersects(CConversionFlags::ZERO_PAD)
        && !spec.flags.intersects(CConversionFlags::LEFT_ADJUST)
        && spec.min_field_width.is_some()
    {
        return true;
    }

    spec.flags
        .intersects(CConversionFlags::BLANK_SIGN | CConversionFlags::SIGN_CHAR)
        || spec.precision.is_some()
}
