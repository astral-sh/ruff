use rustpython_parser::ast::Location;

use crate::checks::{Check, CheckKind};
use crate::settings::Settings;

/// Whether the given line is too long and should be reported.
fn should_enforce_line_length(line: &str, limit: usize) -> bool {
    if line.len() > limit {
        let mut chunks = line.split_whitespace();
        if let (Some(first), Some(_)) = (chunks.next(), chunks.next()) {
            // Do not enforce the line length for commented lines with a single word
            !(first == "#" && chunks.next().is_none())
        } else {
            // Single word / no printable chars - no way to make the line shorter
            false
        }
    } else {
        false
    }
}

pub fn check_lines(checks: &mut Vec<Check>, contents: &str, settings: &Settings) {
    let enforce_line_too_long = settings.select.contains(CheckKind::LineTooLong.code());

    let mut line_checks = vec![];
    let mut ignored = vec![];
    for (row, line) in contents.lines().enumerate() {
        // Remove any ignored checks.
        // TODO(charlie): Only validate checks for the current line.
        for (index, check) in checks.iter().enumerate() {
            if check.location.row() == row + 1 && check.is_inline_ignored(line) {
                ignored.push(index);
            }
        }

        // Enforce line length.
        if enforce_line_too_long && should_enforce_line_length(line, settings.line_length) {
            let check = Check::new(
                CheckKind::LineTooLong,
                Location::new(row + 1, settings.line_length + 1),
            );
            if !check.is_inline_ignored(line) {
                line_checks.push(check);
            }
        }
    }
    ignored.sort();
    for index in ignored.iter().rev() {
        checks.swap_remove(*index);
    }
    checks.extend(line_checks);
}
