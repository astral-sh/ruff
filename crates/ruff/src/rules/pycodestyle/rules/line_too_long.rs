use rustpython_parser::ast::Location;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::rules::pycodestyle::helpers::is_overlong;
use crate::settings::Settings;

/// ## What it does
/// Checks if all lines stay in the specified maximum character length.
///
/// ## Why is this bad?
/// There are still many devices around that are limited to 80 character
/// lines; plus, limiting windows to 80 characters makes it possible to
/// have several windows side-by-side. The default wrapping on such
/// devices looks ugly. Therefore, please limit all lines to a maximum
/// of 79 characters. For flowing long blocks of text (docstrings or
/// comments), limiting the length to 72 characters is recommended.
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
