use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for `string.format()` in logging statements.
///
/// ## Why is this bad?
/// Formatting strings in logging statements can produce inconsistent logging
/// outputs that are difficult to read and error-prone (namely, regarding
/// accidentally logging sensitive information).
///
/// Additionally, formatting the string directly means formatting happens even
/// if the logging statement is never executed (e.g., if the log level is above
/// the level of the logging statement).
///
/// Instead, use the `extra` keyword argument to `logging` methods and define
/// an explicit format in the logger configuration. This is more consistent
/// and efficient, and less error-prone.
///
/// Or, to avoid using the `extra` keyword argument, pass the values to be
/// logged as arguments to the logging method so that string formatting is
/// deferred until required.
///
/// ## Example
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(message)s", level=logging.INFO)
///
/// user = "Maria"
///
/// logging.info("{} - Something happened".format(user))
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(user_id)s - %(message)s", level=logging.INFO)
///
/// user = "Maria"
///
/// logging.info("Something happened", extra=dict(user_id=user))
/// ```
///
/// Or, to avoid using the `extra` keyword argument:
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
/// ## References
/// - [Python documentation: `logging`](https://docs.python.org/3/library/logging.html)
/// - [Python documentation: Optimization](https://docs.python.org/3/howto/logging.html#optimization)
/// - [Python documentation: Logging variable data](https://docs.python.org/3/howto/logging.html#logging-variable-data)
#[violation]
pub struct LoggingStringFormat;

impl Violation for LoggingStringFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `string.format()`")
    }
}

/// ## What it does
/// Checks for `printf`-style format strings in logging statements.
///
/// ## Why is this bad?
/// Formatting strings in logging statements can produce inconsistent logging
/// outputs that are difficult to read and error-prone (namely, regarding
/// accidentally logging sensitive information).
///
/// Additionally, formatting the string directly means formatting happens even
/// if the logging statement is never executed (e.g., if the log level is above
/// the level of the logging statement).
///
/// Instead, use the `extra` keyword argument to `logging` methods and define
/// an explicit format in the logger configuration. This is more consistent
/// and efficient, and less error-prone.
///
/// Or, to avoid using the `extra` keyword argument, pass the values to be
/// logged as arguments to the logging method so that string formatting is
/// deferred until required.
///
/// ## Example
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(message)s", level=logging.INFO)
///
/// user = "Maria"
///
/// logging.info("%s - Something happened" % user)
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(user_id)s - %(message)s", level=logging.INFO)
///
/// user = "Maria"
///
/// logging.info("Something happened", extra=dict(user_id=user))
/// ```
///
/// Or, to avoid using the `extra` keyword argument:
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
/// ## References
/// - [Python documentation: `logging`](https://docs.python.org/3/library/logging.html)
/// - [Python documentation: Optimization](https://docs.python.org/3/howto/logging.html#optimization)
/// - [Python documentation: Logging variable data](https://docs.python.org/3/howto/logging.html#logging-variable-data)
#[violation]
pub struct LoggingPercentFormat;

impl Violation for LoggingPercentFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `%`")
    }
}

/// ## What it does
/// Checks for string concatenation using the `+` operator in logging
/// statements.
///
/// ## Why is this bad?
/// Formatting strings in logging statements can produce inconsistent logging
/// outputs that are difficult to read and error-prone (namely, regarding
/// accidentally logging sensitive information).
///
/// Additionally, formatting the string directly means formatting happens even
/// if the logging statement is never executed (e.g., if the log level is above
/// the level of the logging statement).
///
/// Instead, use the `extra` keyword argument to `logging` methods and define
/// an explicit format in the logger configuration. This is more consistent
/// and efficient, and less error-prone.
///
/// Or, to avoid using the `extra` keyword argument, pass the values to be
/// logged as arguments to the logging method so that string formatting is
/// deferred until required.
///
/// ## Example
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(message)s", level=logging.INFO)
///
/// user = "Maria"
///
/// logging.info(user + " - Something happened")
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(user_id)s - %(message)s", level=logging.INFO)
///
/// user = "Maria"
///
/// logging.info("Something happened", extra=dict(user_id=user))
/// ```
///
/// Or, to avoid using the `extra` keyword argument:
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
/// ## References
/// - [Python documentation: `logging`](https://docs.python.org/3/library/logging.html)
/// - [Python documentation: Optimization](https://docs.python.org/3/howto/logging.html#optimization)
/// - [Python documentation: Logging variable data](https://docs.python.org/3/howto/logging.html#logging-variable-data)
#[violation]
pub struct LoggingStringConcat;

impl Violation for LoggingStringConcat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `+`")
    }
}

/// ## What it does
/// Checks for f-strings in logging statements.
///
/// ## Why is this bad?
/// Formatting strings in logging statements can produce inconsistent logging
/// outputs that are difficult to read and error-prone (namely, regarding
/// accidentally logging sensitive information).
///
/// Additionally, formatting the string directly means formatting happens even
/// if the logging statement is never executed (e.g., if the log level is above
/// the level of the logging statement).
///
/// Instead, use the `extra` keyword argument to `logging` methods and define
/// an explicit format in the logger configuration. This is more consistent
/// and efficient, and less error-prone.
///
/// Or, to avoid using the `extra` keyword argument, pass the values to be
/// logged as arguments to the logging method so that string formatting is
/// deferred until required.
///
/// ## Example
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(message)s", level=logging.INFO)
///
/// user = "Maria"
///
/// logging.info(f"{user} - Something happened")
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(user_id)s - %(message)s", level=logging.INFO)
///
/// user = "Maria"
///
/// logging.info("Something happened", extra=dict(user_id=user))
/// ```
///
/// Or, to avoid using the `extra` keyword argument:
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
/// ## References
/// - [Python documentation: `logging`](https://docs.python.org/3/library/logging.html)
/// - [Python documentation: Optimization](https://docs.python.org/3/howto/logging.html#optimization)
/// - [Python documentation: Logging variable data](https://docs.python.org/3/howto/logging.html#logging-variable-data)
#[violation]
pub struct LoggingFString;

impl Violation for LoggingFString {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses f-string")
    }
}

/// ## What it does
/// Checks for uses of `logging.warn` and `logging.Logger.warn`.
///
/// ## Why is this bad?
/// `logging.warn` and `logging.Logger.warn` are deprecated in favor of
/// `logging.warning` and `logging.Logger.warning`, which are functionally
/// equivalent.
///
/// ## Example
/// ```python
/// import logging
///
/// logging.warn("Something happened")
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// logging.warning("Something happened")
/// ```
///
/// ## References
/// - [Python documentation: `logging.warning`](https://docs.python.org/3/library/logging.html#logging.warning)
/// - [Python documentation: `warning`](https://docs.python.org/3/library/logging.html#logging.Logger.warning)
#[violation]
pub struct LoggingWarn;

impl AlwaysAutofixableViolation for LoggingWarn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `warn` instead of `warning`")
    }

    fn autofix_title(&self) -> String {
        "Convert to `warn`".to_string()
    }
}

/// ## What it does
/// Checks for `extra` keywords in logging statements that clash with
/// `LogRecord` attributes.
///
/// ## Why is this bad?
/// The `extra` argument to `logging` methods and the `LogRecord` attributes
/// are both used to add additional information to logging statements. If any
/// fields clash, a `KeyError` will be raised.
///
/// ## Example
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(name) - %(message)s", level=logging.INFO)
///
/// username = "Maria"
///
/// logging.info("Something happened", extra=dict(name=username))
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// logging.basicConfig(format="%(user_id)s - %(message)s", level=logging.INFO)
///
/// username = "Maria"
///
/// logging.info("Something happened", extra=dict(user=username))
/// ```
///
/// ## References
/// - [Python documentation: LogRecord attributes](https://docs.python.org/3/library/logging.html#logrecord-attributes)
#[violation]
pub struct LoggingExtraAttrClash(pub String);

impl Violation for LoggingExtraAttrClash {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoggingExtraAttrClash(key) = self;
        format!(
            "Logging statement uses an extra field that clashes with a LogRecord field: `{key}`"
        )
    }
}

/// ## What it does
/// Checks for `.error(..., exc_info=True)` in logging statements.
///
/// ## Why is this bad?
/// `.exception` is equivalent to `.error(..., exc_info=True)`, and is more
/// readable and conveys the intent of the logging statement more clearly.
///
/// ## Example
/// ```python
/// import logging
///
/// try:
///     ...
/// except ValueError:
///     logging.error("Exception occurred", exc_info=True)
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// try:
///     ...
/// except ValueError:
///     logging.exception("Exception occurred")
/// ```
///
/// ## References
/// - [Python documentation: `logging.exception`](https://docs.python.org/3/library/logging.html#logging.exception)
/// - [Python documentation: `exception`](https://docs.python.org/3/library/logging.html#logging.Logger.exception)
/// - [Python documentation: `logging.error`](https://docs.python.org/3/library/logging.html#logging.error)
/// - [Python documentation: `error`](https://docs.python.org/3/library/logging.html#logging.Logger.error)
#[violation]
pub struct LoggingExcInfo;

impl Violation for LoggingExcInfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging `.exception(...)` should be used instead of `.error(..., exc_info=True)`")
    }
}

/// ## What it does
/// Checks for redundant `exc_info` keyword arguments in logging statements.
///
/// ## Why is this bad?
/// `exc_info` is `True` by default for `.exception`, and `False` by  default
/// for `.error`. If `exc_info` is explicitly set to `True` for `.exception` or
/// `False` for `.error`, it is redundant and should be removed.
///
/// ## Example
/// ```python
/// import logging
///
/// try:
///     ...
/// except ValueError:
///     logging.exception("Exception occurred", exc_info=True)
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// try:
///     ...
/// except ValueError:
///     logging.exception("Exception occurred")
/// ```
///
/// ## References
/// - [Python documentation: `logging.exception`](https://docs.python.org/3/library/logging.html#logging.exception)
/// - [Python documentation: `exception`](https://docs.python.org/3/library/logging.html#logging.Logger.exception)
/// - [Python documentation: `logging.error`](https://docs.python.org/3/library/logging.html#logging.error)
/// - [Python documentation: `error`](https://docs.python.org/3/library/logging.html#logging.Logger.error)
#[violation]
pub struct LoggingRedundantExcInfo;

impl Violation for LoggingRedundantExcInfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement has redundant `exc_info`")
    }
}
