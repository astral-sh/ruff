use rustpython_parser::ast::Location;

use crate::checks::{Check, CheckKind};
use crate::settings::Settings;

pub fn check_lines(checks: &mut Vec<Check>, contents: &str, settings: &Settings) {
    let enforce_line_too_ling = settings.select.contains(CheckKind::LineTooLong.code());

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
        if enforce_line_too_ling && line.len() > settings.line_length {
            let chunks: Vec<&str> = line.split_whitespace().collect();
            if !(chunks.len() == 1 || (chunks.len() == 2 && chunks[0] == "#")) {
                let check = Check {
                    kind: CheckKind::LineTooLong,
                    location: Location::new(row + 1, settings.line_length + 1),
                };
                if !check.is_inline_ignored(line) {
                    line_checks.push(check);
                }
            }
        }
    }
    for index in ignored.iter().rev() {
        checks.remove(*index);
    }
    checks.extend(line_checks);
}
