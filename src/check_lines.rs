//! Lint rules based on checking raw physical lines.

use nohash_hasher::IntMap;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::noqa;
use crate::noqa::Directive;
use crate::settings::Settings;
use textwrap::wrap;

// Regex from PEP263
static CODING_COMMENT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[ \t\f]*#.*?coding[:=][ \t]*utf-?8").expect("Invalid regex"));

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


fn add_fix_if_can_fix(check: &mut Check, line: &str, autofix: bool, line_length: usize, target_line_length: usize) {
    if !autofix {
        return
    }
    if line_length <= target_line_length {
        return
    }
    if line.trim_start().starts_with("\"\"\"") {
        let mut whitespace = line.chars().take_while(|c| c.is_whitespace()).collect::<String>();
        let wrap_length = target_line_length - whitespace.len();
        let wrapped = wrap(line, wrap_length);
        whitespace.insert(0, '\n');
        let wrapped = wrapped.join(&whitespace);
        check.fix = Some(Fix::replacement(wrapped, check.location, check.end_location));
    }
}


pub fn check_lines(
    checks: &mut Vec<Check>,
    contents: &str,
    noqa_line_for: &IntMap<usize, usize>,
    settings: &Settings,
    autofix: bool,
) {
    let enforce_unnecessary_coding_comment = settings.enabled.contains(&CheckCode::U009);
    let enforce_line_too_long = settings.enabled.contains(&CheckCode::E501);
    let enforce_noqa = settings.enabled.contains(&CheckCode::M001);

    let mut noqa_directives: IntMap<usize, (Directive, Vec<&str>)> = IntMap::default();
    let mut line_checks = vec![];
    let mut ignored = vec![];

    checks.sort_by_key(|check| check.location);
    let mut checks_iter = checks.iter().enumerate().peekable();
    if let Some((_index, check)) = checks_iter.peek() {
        assert!(check.location.row() >= 1);
    }

    let lines: Vec<&str> = contents.lines().collect();
    for (lineno, line) in lines.iter().enumerate() {
        // Grab the noqa (logical) line number for the current (physical) line.
        // If there are newlines at the end of the file, they won't be represented in
        // `noqa_line_for`, so fallback to the current line.
        let noqa_lineno = noqa_line_for.get(&(lineno + 1)).unwrap_or(&(lineno + 1)) - 1;

        // Enforce unnecessary coding comments (U009).
        if enforce_unnecessary_coding_comment {
            if lineno < 2 {
                // PEP3120 makes utf-8 the default encoding.
                if CODING_COMMENT_REGEX.is_match(line) {
                    let line_length = line.len();
                    let mut check = Check::new(
                        CheckKind::PEP3120UnnecessaryCodingComment,
                        Range {
                            location: Location::new(lineno + 1, 0),
                            end_location: Location::new(lineno + 1, line_length + 1),
                        },
                    );
                    if autofix && settings.fixable.contains(check.kind.code()) {
                        check.amend(Fix::deletion(
                            Location::new(lineno + 1, 0),
                            Location::new(lineno + 1, line_length + 1),
                        ));
                    }
                    line_checks.push(check);
                }
            }
        }

        if enforce_noqa {
            noqa_directives
                .entry(noqa_lineno)
                .or_insert_with(|| (noqa::extract_noqa_directive(lines[noqa_lineno]), vec![]));
        }

        // Remove any ignored checks.
        while let Some((index, check)) =
            checks_iter.next_if(|(_index, check)| check.location.row() == lineno + 1)
        {
            let noqa = noqa_directives
                .entry(noqa_lineno)
                .or_insert_with(|| (noqa::extract_noqa_directive(lines[noqa_lineno]), vec![]));

            match noqa {
                (Directive::All(..), matches) => {
                    matches.push(check.kind.code().as_ref());
                    ignored.push(index);
                }
                (Directive::Codes(.., codes), matches) => {
                    if codes.contains(&check.kind.code().as_ref()) {
                        matches.push(check.kind.code().as_ref());
                        ignored.push(index);
                    }
                }
                (Directive::None, _) => {}
            }
        }
        // Enforce line length violations (E501).
        if enforce_line_too_long {
            let line_length = line.chars().count();
            if should_enforce_line_length(line, line_length, settings.line_length) {
                let noqa = noqa_directives
                    .entry(noqa_lineno)
                    .or_insert_with(|| (noqa::extract_noqa_directive(lines[noqa_lineno]), vec![]));

                let mut check = Check::new(
                    CheckKind::LineTooLong(line_length, settings.line_length),
                    Range {
                        location: Location::new(lineno + 1, 0),
                        end_location: Location::new(lineno + 1, line_length),
                    },
                );
                add_fix_if_can_fix(&mut check, line, autofix, line_length, settings.line_length);

                match noqa {
                    (Directive::All(..), matches) => {
                        matches.push(check.kind.code().as_ref());
                    }
                    (Directive::Codes(.., codes), matches) => {
                        if codes.contains(&check.kind.code().as_ref()) {
                            matches.push(check.kind.code().as_ref());
                        } else {
                            line_checks.push(check);
                        }
                    }
                    (Directive::None, _) => line_checks.push(check),
                }
            }
        }
    }

    // Enforce newlines at end of files.
    if settings.enabled.contains(&CheckCode::W292) && !contents.ends_with('\n') {
        // Note: if `lines.last()` is `None`, then `contents` is empty (and so we don't
        // want to raise W292 anyway).
        if let Some(line) = lines.last() {
            let lineno = lines.len() - 1;
            let noqa_lineno = noqa_line_for.get(&(lineno + 1)).unwrap_or(&(lineno + 1)) - 1;

            let noqa = noqa_directives
                .entry(noqa_lineno)
                .or_insert_with(|| (noqa::extract_noqa_directive(lines[noqa_lineno]), vec![]));

            let check = Check::new(
                CheckKind::NoNewLineAtEndOfFile,
                Range {
                    location: Location::new(lines.len(), line.len() + 1),
                    end_location: Location::new(lines.len(), line.len() + 1),
                },
            );

            match noqa {
                (Directive::All(..), matches) => {
                    matches.push(check.kind.code().as_ref());
                }
                (Directive::Codes(.., codes), matches) => {
                    if codes.contains(&check.kind.code().as_ref()) {
                        matches.push(check.kind.code().as_ref());
                    } else {
                        line_checks.push(check);
                    }
                }
                (Directive::None, _) => line_checks.push(check),
            }
        }
    }

    // Enforce that the noqa directive was actually used.
    if enforce_noqa {
        for (row, (directive, matches)) in noqa_directives {
            match directive {
                Directive::All(spaces, start, end) => {
                    if matches.is_empty() {
                        let mut check = Check::new(
                            CheckKind::UnusedNOQA(None),
                            Range {
                                location: Location::new(row + 1, start),
                                end_location: Location::new(row + 1, end),
                            },
                        );
                        if autofix && settings.fixable.contains(check.kind.code()) {
                            check.amend(Fix::deletion(
                                Location::new(row + 1, start - spaces),
                                Location::new(row + 1, lines[row].chars().count()),
                            ));
                        }
                        line_checks.push(check);
                    }
                }
                Directive::Codes(spaces, start, end, codes) => {
                    let mut invalid_codes = vec![];
                    let mut valid_codes = vec![];
                    for code in codes {
                        if matches.contains(&code) {
                            valid_codes.push(code.to_string());
                        } else {
                            invalid_codes.push(code.to_string());
                        }
                    }

                    if !invalid_codes.is_empty() {
                        let mut check = Check::new(
                            CheckKind::UnusedNOQA(Some(invalid_codes)),
                            Range {
                                location: Location::new(row + 1, start),
                                end_location: Location::new(row + 1, end),
                            },
                        );
                        if autofix && settings.fixable.contains(check.kind.code()) {
                            if valid_codes.is_empty() {
                                check.amend(Fix::deletion(
                                    Location::new(row + 1, start - spaces),
                                    Location::new(row + 1, lines[row].chars().count()),
                                ));
                            } else {
                                check.amend(Fix::replacement(
                                    format!("# noqa: {}", valid_codes.join(", ")),
                                    Location::new(row + 1, start),
                                    Location::new(row + 1, lines[row].chars().count()),
                                ));
                            }
                        }
                        line_checks.push(check);
                    }
                }
                Directive::None => {}
            }
        }
    }

    ignored.sort_unstable();
    for index in ignored.iter().rev() {
        checks.swap_remove(*index);
    }
    checks.extend(line_checks);
}

#[cfg(test)]
mod tests {
    use nohash_hasher::IntMap;

    use super::check_lines;
    use crate::checks::{Check, CheckCode};
    use crate::settings::Settings;

    #[test]
    fn e501_non_ascii_char() {
        let line = "'\u{4e9c}' * 2"; // 7 in UTF-32, 9 in UTF-8.
        let noqa_line_for: IntMap<usize, usize> = IntMap::default();
        let check_with_max_line_length = |line_length: usize| {
            let mut checks: Vec<Check> = vec![];
            check_lines(
                &mut checks,
                line,
                &noqa_line_for,
                &Settings {
                    line_length,
                    ..Settings::for_rule(CheckCode::E501)
                },
                true,
            );
            checks
        };
        assert!(!check_with_max_line_length(6).is_empty());
        assert!(check_with_max_line_length(7).is_empty());
    }

    #[test]
    fn test_fix_field_docstring() {
        let file = r#"
class Account:
    address: str = None
    """Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua."""
"#.trim_start();
        let mut checks = vec![];
        check_lines(
            &mut checks,
            file,
            &IntMap::default(),
            &Settings {
                line_length: 79,
                ..Settings::for_rule(CheckCode::E501)
            },
            true,
        );
        assert_eq!(checks.len(), 1);
        let check = &checks[0];
        assert_eq!(check.kind.code(), &CheckCode::E501);
        let Some(fix) = &check.fix else { panic!("No fix."); };
        assert_eq!(fix.patch.content, r#"
    """Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do
    eiusmod tempor incididunt ut labore et dolore magna aliqua."""
"#.trim_matches('\n'));
    }
}
