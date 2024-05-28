use ruff_diagnostics::{AlwaysFixableViolation, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for uses of `str.format` to format logging messages.
///
/// ## Why is this bad?
/// The `logging` module provides a mechanism for passing additional values to
/// be logged using the `extra` keyword argument. This is more consistent, more
/// efficient, and less error-prone than formatting the string directly.
///
/// Using `str.format` to format a logging message requires that Python eagerly
/// format the string, even if the logging statement is never executed (e.g.,
/// if the log level is above the level of the logging statement), whereas
/// using the `extra` keyword argument defers formatting until required.
///
/// Additionally, the use of `extra` will ensure that the values are made
/// available to all handlers, which can then be configured to log the values
/// in a consistent manner.
///
/// As an alternative to `extra`, passing values as arguments to the logging
/// method can also be used to defer string formatting until required.
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
/// logging.info("Something happened", extra={"user_id": user})
/// ```
///
/// Or:
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
#[violation]
pub struct LoggingStringFormat;

impl Violation for LoggingStringFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `str.format`")
    }
}

/// ## What it does
/// Checks for uses of `printf`-style format strings to format logging
/// messages.
///
/// ## Why is this bad?
/// The `logging` module provides a mechanism for passing additional values to
/// be logged using the `extra` keyword argument. This is more consistent, more
/// efficient, and less error-prone than formatting the string directly.
///
/// Using `printf`-style format strings to format a logging message requires
/// that Python eagerly format the string, even if the logging statement is
/// never executed (e.g., if the log level is above the level of the logging
/// statement), whereas using the `extra` keyword argument defers formatting
/// until required.
///
/// Additionally, the use of `extra` will ensure that the values are made
/// available to all handlers, which can then be configured to log the values
/// in a consistent manner.
///
/// As an alternative to `extra`, passing values as arguments to the logging
/// method can also be used to defer string formatting until required.
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
/// Or:
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
#[violation]
pub struct LoggingPercentFormat;

impl Violation for LoggingPercentFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `%`")
    }
}

/// ## What it does
/// Checks for uses string concatenation via the `+` operator to format logging
/// messages.
///
/// ## Why is this bad?
/// The `logging` module provides a mechanism for passing additional values to
/// be logged using the `extra` keyword argument. This is more consistent, more
/// efficient, and less error-prone than formatting the string directly.
///
/// Using concatenation to format a logging message requires that Python
/// eagerly format the string, even if the logging statement is never executed
/// (e.g., if the log level is above the level of the logging statement),
/// whereas using the `extra` keyword argument defers formatting until required.
///
/// Additionally, the use of `extra` will ensure that the values are made
/// available to all handlers, which can then be configured to log the values
/// in a consistent manner.
///
/// As an alternative to `extra`, passing values as arguments to the logging
/// method can also be used to defer string formatting until required.
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
/// Or:
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
#[violation]
pub struct LoggingStringConcat;

impl Violation for LoggingStringConcat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `+`")
    }
}

/// ## What it does
/// Checks for uses of f-strings to format logging messages.
///
/// ## Why is this bad?
/// The `logging` module provides a mechanism for passing additional values to
/// be logged using the `extra` keyword argument. This is more consistent, more
/// efficient, and less error-prone than formatting the string directly.
///
/// Using f-strings to format a logging message requires that Python eagerly
/// format the string, even if the logging statement is never executed (e.g.,
/// if the log level is above the level of the logging statement), whereas
/// using the `extra` keyword argument defers formatting until required.
///
/// Additionally, the use of `extra` will ensure that the values are made
/// available to all handlers, which can then be configured to log the values
/// in a consistent manner.
///
/// As an alternative to `extra`, passing values as arguments to the logging
/// method can also be used to defer string formatting until required.
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
/// Or:
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
/// ## Options
/// - `lint.logger-objects`
///
/// ## References
/// - [Python documentation: `logging.warning`](https://docs.python.org/3/library/logging.html#logging.warning)
/// - [Python documentation: `logging.Logger.warning`](https://docs.python.org/3/library/logging.html#logging.Logger.warning)
#[violation]
pub struct LoggingWarn;

impl AlwaysFixableViolation for LoggingWarn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `warn` instead of `warning`")
    }

    fn fix_title(&self) -> String {
        "Convert to `warning`".to_string()
    }
}

/// ## What it does
/// Checks for `extra` keywords in logging statements that clash with
/// `LogRecord` attributes.
///
/// ## Why is this bad?
/// The `logging` module provides a mechanism for passing additional values to
/// be logged using the `extra` keyword argument. These values are then passed
/// to the `LogRecord` constructor.
///
/// Providing a value via `extra` that clashes with one of the attributes of
/// the `LogRecord` constructor will raise a `KeyError` when the `LogRecord` is
/// constructed.
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
/// ## Options
/// - `lint.logger-objects`
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
            "Logging statement uses an `extra` field that clashes with a `LogRecord` field: `{key}`"
        )
    }
}

/// ## What it does
/// Checks for uses of `logging.error` that pass `exc_info=True`.
///
/// ## Why is this bad?
/// Calling `logging.error` with `exc_info=True` is equivalent to calling
/// `logging.exception`. Using `logging.exception` is more concise, more
/// readable, and conveys the intent of the logging statement more clearly.
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
/// ## Options
/// - `lint.logger-objects`
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
/// `exc_info` is `True` by default for `logging.exception`, and `False` by
/// default for `logging.error`.
///
/// Passing `exc_info=True` to `logging.exception` calls is redundant, as is
/// passing `exc_info=False` to `logging.error` calls.
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
/// ## Options
/// - `lint.logger-objects`
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
