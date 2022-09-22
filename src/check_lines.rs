use std::collections::BTreeMap;

use rustpython_parser::ast::Location;

use crate::checks::{Check, CheckCode, CheckKind};
use crate::noqa;
use crate::noqa::Directive;
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

pub fn check_lines(
    checks: &mut Vec<Check>,
    contents: &str,
    noqa_line_for: &[usize],
    settings: &Settings,
) {
    let enforce_line_too_long = settings.select.contains(&CheckCode::E501);
    let enforce_noqa = settings.select.contains(&CheckCode::M001);

    let mut noqa_directives: BTreeMap<usize, (Directive, Vec<&str>)> = BTreeMap::new();

    let mut line_checks = vec![];
    let mut ignored = vec![];

    let lines: Vec<&str> = contents.lines().collect();
    for (lineno, line) in lines.iter().enumerate() {
        // Grab the noqa (logical) line number for the current (physical) line.
        // If there are newlines at the end of the file, they won't be represented in
        // `noqa_line_for`, so fallback to the current line.
        let noqa_lineno = noqa_line_for
            .get(lineno)
            .map(|lineno| lineno - 1)
            .unwrap_or(lineno);

        if enforce_noqa {
            noqa_directives
                .entry(noqa_lineno)
                .or_insert_with(|| (noqa::extract_noqa_directive(lines[noqa_lineno]), vec![]));
        }

        // Remove any ignored checks.
        // TODO(charlie): Only validate checks for the current line.
        for (index, check) in checks.iter().enumerate() {
            if check.location.row() == lineno + 1 {
                let noqa = noqa_directives
                    .entry(noqa_lineno)
                    .or_insert_with(|| (noqa::extract_noqa_directive(lines[noqa_lineno]), vec![]));

                match noqa {
                    (Directive::All(_), matches) => {
                        matches.push(check.kind.code().as_str());
                        ignored.push(index)
                    }
                    (Directive::Codes(_, codes), matches) => {
                        if codes.contains(&check.kind.code().as_str()) {
                            matches.push(check.kind.code().as_str());
                            ignored.push(index);
                        }
                    }
                    (Directive::None, _) => {}
                }
            }
        }

        // Enforce line length.
        if enforce_line_too_long {
            let line_length = line.chars().count();
            if should_enforce_line_length(line, line_length, settings.line_length) {
                let noqa = noqa_directives
                    .entry(noqa_lineno)
                    .or_insert_with(|| (noqa::extract_noqa_directive(lines[noqa_lineno]), vec![]));

                let check = Check::new(
                    CheckKind::LineTooLong(line_length, settings.line_length),
                    Location::new(lineno + 1, settings.line_length + 1),
                );

                match noqa {
                    (Directive::All(_), matches) => {
                        matches.push(check.kind.code().as_str());
                    }
                    (Directive::Codes(_, codes), matches) => {
                        if codes.contains(&check.kind.code().as_str()) {
                            matches.push(check.kind.code().as_str());
                        } else {
                            line_checks.push(check);
                        }
                    }
                    (Directive::None, _) => line_checks.push(check),
                }
            }
        }
    }

    // Enforce that the noqa directive was actually used.
    if enforce_noqa {
        for (row, (directive, matches)) in noqa_directives {
            match directive {
                Directive::All(column) => {
                    if matches.is_empty() {
                        line_checks.push(Check::new(
                            CheckKind::UnusedNOQA(None),
                            Location::new(row + 1, column + 1),
                        ));
                    }
                }
                Directive::Codes(column, codes) => {
                    for code in &codes {
                        if !matches.contains(code) {
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
        let noqa_line_for: Vec<usize> = vec![1];
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

            check_lines(&mut checks, line, &noqa_line_for, &settings);
            return checks;
        };
        assert!(!check_with_max_line_length(6).is_empty());
        assert!(check_with_max_line_length(7).is_empty());
    }
}
