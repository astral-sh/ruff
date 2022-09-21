use once_cell::sync::Lazy;
use rustpython_parser::ast::Location;

use crate::checks::{extract_noqa_directive, Check, CheckCode, CheckKind, Directive};
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
    let enforce_noqa = settings.select.contains(&CheckCode::M001);

    let mut line_checks = vec![];
    let mut ignored = vec![];
    for (row, line) in contents.lines().enumerate() {
        let noqa_directive = Lazy::new(|| extract_noqa_directive(line));
        let mut line_ignored: Vec<&str> = vec![];

        // Remove any ignored checks.
        // TODO(charlie): Only validate checks for the current line.
        for (index, check) in checks.iter().enumerate() {
            if check.location.row() == row + 1 {
                match &*noqa_directive {
                    Directive::All(_) => {
                        line_ignored.push(check.kind.code().as_str());
                        ignored.push(index)
                    }
                    Directive::Codes(_, codes) => {
                        if codes.contains(&check.kind.code().as_str()) {
                            line_ignored.push(check.kind.code().as_str());
                            ignored.push(index);
                        }
                    }
                    Directive::None => {}
                }
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
                match &*noqa_directive {
                    Directive::All(_) => {
                        line_ignored.push(check.kind.code().as_str());
                    }
                    Directive::Codes(_, codes) => {
                        if codes.contains(&check.kind.code().as_str()) {
                            line_ignored.push(check.kind.code().as_str());
                        } else {
                            line_checks.push(check);
                        }
                    }
                    Directive::None => line_checks.push(check),
                }
            }
        }

        // Enforce that the noqa was actually used.
        if enforce_noqa {
            match &*noqa_directive {
                Directive::All(column) => {
                    if line_ignored.is_empty() {
                        line_checks.push(Check::new(
                            CheckKind::UnusedNOQA(None),
                            Location::new(row + 1, column + 1),
                        ));
                    }
                }
                Directive::Codes(column, codes) => {
                    for code in codes {
                        if !line_ignored.contains(code) {
                            line_checks.push(Check::new(
                                CheckKind::UnusedNOQA(Some(code.to_string())),
                                Location::new(row + 1, column + 1),
                            ));
                        }
                    }
                }
                Directive::None => {}
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
    use std::collections::BTreeSet;

    use super::check_lines;
    use super::*;

    #[test]
    fn e501_non_ascii_char() {
        let line = "'\u{4e9c}' * 2"; // 7 in UTF-32, 9 in UTF-8.
        let check_with_max_line_length = |line_length: usize| {
            let mut checks: Vec<Check> = vec![];
            let settings = Settings {
                pyproject: None,
                project_root: None,
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
