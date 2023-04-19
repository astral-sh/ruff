use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::rules::pycodestyle::helpers::is_overlong;
use crate::settings::Settings;

/// ## What it does
/// Checks for lines that exceed the specified maximum character length.
///
/// ## Why is this bad?
/// Overlong lines can hurt readability.
///
/// ## Example
/// ```python
/// my_function(param1, param2, param3, param4, param5, param6, param7, param8, param9, param10)
/// ```
///
/// Use instead:
/// ```python
/// my_function(
///     param1, param2, param3, param4, param5,
///     param6, param7, param8, param9, param10
/// )
/// ```
#[violation]
pub struct LineTooLong(pub usize, pub usize);

impl Violation for LineTooLong {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LineTooLong(width, limit) = self;
        format!("Line too long ({width} > {limit} characters)")
    }
}

/// E501
pub fn line_too_long(lineno: usize, line: &str, settings: &Settings) -> Option<Diagnostic> {
    let limit = settings.line_length;

    is_overlong(
        line,
        limit,
        settings.pycodestyle.ignore_overlong_task_comments,
        &settings.task_tags,
    )
    .map(|overlong| Diagnostic::new(LineTooLong(overlong.width(), limit), overlong.range(lineno)))
}
