use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct LoggingStringFormat;

impl Violation for LoggingStringFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `string.format()`")
    }
}

#[violation]
pub struct LoggingPercentFormat;

impl Violation for LoggingPercentFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `%`")
    }
}

#[violation]
pub struct LoggingStringConcat;

impl Violation for LoggingStringConcat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `+`")
    }
}

#[violation]
pub struct LoggingFString;

impl Violation for LoggingFString {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses f-string")
    }
}

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

#[violation]
pub struct LoggingExcInfo;

impl Violation for LoggingExcInfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging `.exception(...)` should be used instead of `.error(..., exc_info=True)`")
    }
}

#[violation]
pub struct LoggingRedundantExcInfo;

impl Violation for LoggingRedundantExcInfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement has redundant `exc_info`")
    }
}
