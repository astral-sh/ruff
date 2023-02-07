use ruff_macros::{define_violation, derive_message_formats};

use crate::violation::{AlwaysAutofixableViolation, Violation};

define_violation!(
    pub struct LoggingStringFormat;
);
impl Violation for LoggingStringFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `string.format()`")
    }
}

define_violation!(
    pub struct LoggingPercentFormat;
);
impl Violation for LoggingPercentFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `%`")
    }
}

define_violation!(
    pub struct LoggingStringConcat;
);
impl Violation for LoggingStringConcat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `+`")
    }
}

define_violation!(
    pub struct LoggingFString;
);
impl Violation for LoggingFString {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses f-string")
    }
}

define_violation!(
    pub struct LoggingWarn;
);
impl AlwaysAutofixableViolation for LoggingWarn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement uses `warn` instead of `warning`")
    }

    fn autofix_title(&self) -> String {
        "Convert to `warn`".to_string()
    }
}

define_violation!(
    pub struct LoggingExtraAttrClash(pub String);
);
impl Violation for LoggingExtraAttrClash {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoggingExtraAttrClash(key) = self;
        format!(
            "Logging statement uses an extra field that clashes with a LogRecord field: `{key}`"
        )
    }
}

define_violation!(
    pub struct LoggingExcInfo;
);
impl Violation for LoggingExcInfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging `.exception(...)` should be used instead of `.error(..., exc_info=True)`")
    }
}

define_violation!(
    pub struct LoggingRedundantExcInfo;
);
impl Violation for LoggingRedundantExcInfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Logging statement has redundant `exc_info`")
    }
}
