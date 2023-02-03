use ruff_macros::derive_message_formats;

use crate::violation::{AlwaysAutofixableViolation, Violation};
use crate::{define_simple_autofix_violation, define_simple_violation, define_violation};

define_simple_violation!(
    LoggingStringFormat,
    "Logging statement uses `string.format()`"
);

define_simple_violation!(LoggingPercentFormat, "Logging statement uses `%`");

define_simple_violation!(LoggingStringConcat, "Logging statement uses `+`");

define_simple_violation!(LoggingFString, "Logging statement uses f-string");

define_simple_autofix_violation!(
    LoggingWarn,
    "Logging statement uses `warn` instead of `warning`",
    "Convert to `warn`"
);

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

define_simple_violation!(
    LoggingExcInfo,
    "Logging `.exception(...)` should be used instead of `.error(..., exc_info=True)`"
);

define_simple_violation!(
    LoggingRedundantExcInfo,
    "Logging statement has redundant `exc_info`"
);
