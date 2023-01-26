use rustpython_ast::Location;

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::rules::pycodestyle::helpers::is_overlong;
use crate::settings::Settings;
use crate::violations;

/// W505
pub fn doc_line_too_long(lineno: usize, line: &str, settings: &Settings) -> Option<Diagnostic> {
    let Some(limit) = settings.pycodestyle.max_doc_length else {
        return None;
    };

    let line_length = line.chars().count();
    if is_overlong(
        line,
        line_length,
        limit,
        settings.pycodestyle.ignore_overlong_task_comments,
        &settings.task_tags,
    ) {
        Some(Diagnostic::new(
            violations::DocLineTooLong(line_length, limit),
            Range::new(
                Location::new(lineno + 1, limit),
                Location::new(lineno + 1, line_length),
            ),
        ))
    } else {
        None
    }
}
