use nohash_hasher::IntMap;
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checks::{Check, CheckCode, CheckKind, CODE_REDIRECTS};
use crate::noqa;
use crate::noqa::{is_file_exempt, Directive};
use crate::settings::{flags, Settings};

pub fn check_noqa(
    checks: &mut Vec<Check>,
    contents: &str,
    noqa_line_for: &IntMap<usize, usize>,
    settings: &Settings,
    autofix: flags::Autofix,
) {
    let enforce_noqa = settings.enabled.contains(&CheckCode::RUF100);

    let mut noqa_directives: IntMap<usize, (Directive, Vec<&str>)> = IntMap::default();
    let mut ignored = vec![];

    checks.sort_by_key(|check| check.location);
    let mut checks_iter = checks.iter().enumerate().peekable();
    if let Some((_index, check)) = checks_iter.peek() {
        assert!(check.location.row() >= 1);
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
                        if matches!(autofix, flags::Autofix::Enabled)
                            && settings.fixable.contains(check.kind.code())
                        {
                            check.amend(Fix::deletion(
                                Location::new(row + 1, start - spaces),
                                Location::new(row + 1, lines[row].chars().count()),
                            ));
                        }
                        checks.push(check);
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
                        if matches!(autofix, flags::Autofix::Enabled)
                            && settings.fixable.contains(check.kind.code())
                        {
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
                        checks.push(check);
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
}
