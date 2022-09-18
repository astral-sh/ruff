use rustpython_parser::ast::Location;

use crate::checks::{Check, CheckCode, CheckKind};
use crate::settings::Settings;

/// Whether the given line is too long and should be reported.
fn should_enforce_line_length(line: &str, length: usize, limit: usize) -> bool {
    if length > limit {
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
    let enforce_line_too_long = settings.select.contains(&CheckCode::E501);

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
        if enforce_line_too_long {
            let line_length = line.chars().count();
            if should_enforce_line_length(line, line_length, settings.line_length) {
                let check = Check::new(
                    CheckKind::LineTooLong(line_length, settings.line_length),
                    Location::new(row + 1, settings.line_length + 1),
                );
                if !check.is_inline_ignored(line) {
                    line_checks.push(check);
                }
            }
        }
    }
    ignored.sort();
    for index in ignored.iter().rev() {
        checks.swap_remove(*index);
    }
    checks.extend(line_checks);
}

#[cfg(test)]
mod tests {
    use super::check_lines;
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn e501_non_ascii_char() {
        let line = "'\u{4e9c}' * 2"; // 7 in UTF-32, 9 in UTF-8.
        let check_with_max_line_length = |line_length: usize| {
            let mut checks: Vec<Check> = vec![];
            let settings = Settings {
                line_length,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from_iter(vec![CheckCode::E501]),
            };
            check_lines(&mut checks, line, &settings);
            return checks;
        };
        assert!(!check_with_max_line_length(6).is_empty());
        assert!(check_with_max_line_length(7).is_empty());
    }
}
