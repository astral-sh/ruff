use std::str::FromStr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_literal::cformat::{CFormatPart, CFormatString, CFormatType};
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
}

impl Violation for LoggingEagerConversion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoggingEagerConversion { format_conversion } = self;
        let (format_str, call_arg) = match format_conversion {
            FormatConversion::Str => ("%s", "str()"),
            FormatConversion::Repr => ("%r", "repr()"),
            FormatConversion::Ascii => ("%a", "ascii()"),
            FormatConversion::Bytes => ("%b", "bytes()"),
        };
        format!("Unnecessary `{call_arg}` conversion when formatting with `{format_str}`")
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
        if let Expr::Call(ast::ExprCall { func, .. }) = arg {
            let CFormatType::String(format_conversion) = spec.format_type else {
                continue;
            };

            // Check for use of %s with str()
            if checker.semantic().match_builtin_expr(func.as_ref(), "str")
                && matches!(format_conversion, FormatConversion::Str)
            {
                checker
                    .report_diagnostic(LoggingEagerConversion { format_conversion }, arg.range());
            }
        }
    }
}
