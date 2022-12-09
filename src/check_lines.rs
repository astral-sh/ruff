//! Lint rules based on checking raw physical lines.

use nohash_hasher::IntMap;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checks::{Check, CheckCode, CheckKind, CODE_REDIRECTS};
use crate::noqa;
use crate::noqa::{is_file_exempt, Directive};
use crate::settings::Settings;

// Regex from PEP263.
static CODING_COMMENT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[ \t\f]*#.*?coding[:=][ \t]*utf-?8").unwrap());

static URL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^https?://\S+$").unwrap());

/// Whether the given line is too long and should be reported.
fn should_enforce_line_length(line: &str, length: usize, limit: usize) -> bool {
    if length <= limit {
        return false;
    }
    let mut chunks = line.split_whitespace();
    let (Some(first), Some(_)) = (chunks.next(), chunks.next()) else {
        // Single word / no printable chars - no way to make the line shorter
        return false;
    };

    // Do not enforce the line length for commented lines that end with a URL
    // or contain only a single word.
    !(first == "#" && chunks.last().map_or(true, |c| URL_REGEX.is_match(c)))
}

pub fn check_lines(
    checks: &mut Vec<Check>,
    contents: &str,
    noqa_line_for: &IntMap<usize, usize>,
    settings: &Settings,
    autofix: bool,
    ignore_noqa: bool,
) {
    let enforce_unnecessary_coding_comment = settings.enabled.contains(&CheckCode::UP009);
    let enforce_line_too_long = settings.enabled.contains(&CheckCode::E501);
    let enforce_noqa = settings.enabled.contains(&CheckCode::RUF100);

    let mut noqa_directives: IntMap<usize, (Directive, Vec<&str>)> = IntMap::default();
    let mut line_checks = vec![];
    let mut ignored = vec![];

    checks.sort_by_key(|check| check.location);
    let mut checks_iter = checks.iter().enumerate().peekable();
    if let Some((_index, check)) = checks_iter.peek() {
        assert!(check.location.row() >= 1);
    }

    macro_rules! add_if {
        ($check:expr, $noqa_lineno:expr, $line:expr) => {{
            match noqa_directives
                .entry($noqa_lineno)
                .or_insert_with(|| (noqa::extract_noqa_directive($line), vec![]))
            {
                (Directive::All(..), matches) => {
                    matches.push($check.kind.code().as_ref());
                    if ignore_noqa {
                        line_checks.push($check);
                    }
                }
                (Directive::Codes(.., codes), matches) => {
                    if noqa::includes($check.kind.code(), codes) {
                        matches.push($check.kind.code().as_ref());
                        if ignore_noqa {
                            line_checks.push($check);
                        }
                    } else {
                        line_checks.push($check);
                    }
                }
                (Directive::None, ..) => line_checks.push($check),
            }
        }};
    }

    let lines: Vec<&str> = contents.lines().collect();
    for (lineno, line) in lines.iter().enumerate() {
        // If we hit an exemption for the entire file, bail.
        if is_file_exempt(line) {
            checks.drain(..);
            return;
        }

        // Grab the noqa (logical) line number for the current (physical) line.
        // If there are newlines at the end of the file, they won't be represented in
        // `noqa_line_for`, so fallback to the current line.
        let noqa_lineno = noqa_line_for.get(&(lineno + 1)).unwrap_or(&(lineno + 1)) - 1;

        // Enforce unnecessary coding comments (UP009).
        if enforce_unnecessary_coding_comment {
            if lineno < 2 {
                // PEP3120 makes utf-8 the default encoding.
                if CODING_COMMENT_REGEX.is_match(line) {
                    let mut check = Check::new(
                        CheckKind::PEP3120UnnecessaryCodingComment,
                        Range {
                            location: Location::new(lineno + 1, 0),
                            end_location: Location::new(lineno + 2, 0),
                        },
                    );
                    if autofix && settings.fixable.contains(check.kind.code()) {
                        check.amend(Fix::deletion(
                            Location::new(lineno + 1, 0),
                            Location::new(lineno + 2, 0),
                        ));
                    }
                    add_if!(check, noqa_lineno, lines[noqa_lineno]);
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
                    if noqa::includes(check.kind.code(), codes) {
                        matches.push(check.kind.code().as_ref());
                        ignored.push(index);
                    }
                }
                (Directive::None, ..) => {}
            }
        }

        // Enforce line length violations (E501).
        if enforce_line_too_long {
            let line_length = line.chars().count();
            if should_enforce_line_length(line, line_length, settings.line_length) {
                let check = Check::new(
                    CheckKind::LineTooLong(line_length, settings.line_length),
                    Range {
                        location: Location::new(lineno + 1, 0),
                        end_location: Location::new(lineno + 1, line_length),
                    },
                );
                add_if!(check, noqa_lineno, lines[noqa_lineno]);
            }
        }
    }

    // Enforce newlines at end of files (W292).
    if settings.enabled.contains(&CheckCode::W292) && !contents.ends_with('\n') {
        // Note: if `lines.last()` is `None`, then `contents` is empty (and so we don't
        // want to raise W292 anyway).
        if let Some(line) = lines.last() {
            let check = Check::new(
                CheckKind::NoNewLineAtEndOfFile,
                Range {
                    location: Location::new(lines.len(), line.len() + 1),
                    end_location: Location::new(lines.len(), line.len() + 1),
                },
            );

            let lineno = lines.len() - 1;
            let noqa_lineno = noqa_line_for.get(&(lineno + 1)).unwrap_or(&(lineno + 1)) - 1;
            add_if!(check, noqa_lineno, lines[noqa_lineno]);
        }
    }

    // Enforce that the noqa directive was actually used (RUF100).
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
                        let code = CODE_REDIRECTS.get(code).map_or(code, AsRef::as_ref);
                        if matches.contains(&code) || settings.external.contains(code) {
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

    if !ignore_noqa {
        ignored.sort_unstable();
        for index in ignored.iter().rev() {
            checks.swap_remove(*index);
        }
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
        let check_with_max_line_length = |line_length: usize| {
            let mut checks: Vec<Check> = vec![];
            check_lines(
                &mut checks,
                line,
                &IntMap::default(),
                &Settings {
                    line_length,
                    ..Settings::for_rule(CheckCode::E501)
                },
                true,
                false,
            );
            checks
        };
        assert!(!check_with_max_line_length(6).is_empty());
        assert!(check_with_max_line_length(7).is_empty());
    }
}
