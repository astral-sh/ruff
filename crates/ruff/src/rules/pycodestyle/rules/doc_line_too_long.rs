use rustpython_parser::ast::Location;
use unicode_width::UnicodeWidthStr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::rules::pycodestyle::helpers::is_overlong;
use crate::settings::Settings;

/// ## What it does
/// Checks for doc lines that exceed the specified maximum character length.
///
/// ## Why is this bad?
/// For flowing long blocks of text (docstrings or comments), overlong lines
/// can hurt readability.
///
/// ## Example
/// ```python
/// def function(x):
///     """Lorem ipsum dolor sit amet, consectetur adipiscing elit. Duis auctor purus ut ex fermentum, at maximus est hendrerit."""
/// ```
///
/// Use instead:
/// ```python
/// def function(x):
///     """
///     Lorem ipsum dolor sit amet, consectetur adipiscing elit.
///     Duis auctor purus ut ex fermentum, at maximus est hendrerit.
///     """
/// ```
#[violation]
pub struct DocLineTooLong(pub usize, pub usize);

impl Violation for DocLineTooLong {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DocLineTooLong(width, limit) = self;
        format!("Doc line too long ({width} > {limit} characters)")
    }
}

/// W505
pub fn doc_line_too_long(lineno: usize, line: &str, settings: &Settings) -> Option<Diagnostic> {
    let Some(limit) = settings.pycodestyle.max_doc_length else {
        return None;
    };

    let line_width = line.width();
    if is_overlong(
        line,
        line_width,
        limit,
        settings.pycodestyle.ignore_overlong_task_comments,
        &settings.task_tags,
    ) {
        Some(Diagnostic::new(
            DocLineTooLong(line_width, limit),
            Range::new(
                Location::new(lineno + 1, limit),
                Location::new(lineno + 1, line.chars().count()),
            ),
        ))
    } else {
        None
    }
}
