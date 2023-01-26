use ruff_macros::derive_message_formats;
use rustpython_ast::Location;

use crate::ast::types::Range;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::rules::pycodestyle::helpers::is_overlong;
use crate::settings::Settings;
use crate::violation::Violation;

define_violation!(
    pub struct LineTooLong(pub usize, pub usize);
);
impl Violation for LineTooLong {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LineTooLong(length, limit) = self;
        format!("Line too long ({length} > {limit} characters)")
    }
}

/// E501
pub fn line_too_long(lineno: usize, line: &str, settings: &Settings) -> Option<Diagnostic> {
    let line_length = line.chars().count();
    let limit = settings.line_length;
    if is_overlong(
        line,
        line_length,
        limit,
        settings.pycodestyle.ignore_overlong_task_comments,
        &settings.task_tags,
    ) {
        Some(Diagnostic::new(
            LineTooLong(line_length, limit),
            Range::new(
                Location::new(lineno + 1, limit),
                Location::new(lineno + 1, line_length),
            ),
        ))
    } else {
        None
    }
}
