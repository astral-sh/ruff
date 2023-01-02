//! `NoQA` enforcement and validation.

use std::str::FromStr;

use nohash_hasher::IntMap;
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::noqa;
use crate::noqa::{is_file_exempt, Directive};
use crate::registry::{Check, CheckCode, CheckKind, UnusedCodes, CODE_REDIRECTS};
use crate::settings::{flags, Settings};

pub fn check_noqa(
    checks: &mut Vec<Check>,
    contents: &str,
    commented_lines: &[usize],
    noqa_line_for: &IntMap<usize, usize>,
    settings: &Settings,
    autofix: flags::Autofix,
) {
    let mut noqa_directives: IntMap<usize, (Directive, Vec<&str>)> = IntMap::default();
    let mut ignored = vec![];

    let enforce_noqa = settings.enabled.contains(&CheckCode::RUF100);

    let lines: Vec<&str> = contents.lines().collect();
    for lineno in commented_lines {
        // If we hit an exemption for the entire file, bail.
        if is_file_exempt(lines[lineno - 1]) {
            checks.drain(..);
            return;
        }

        if enforce_noqa {
            noqa_directives
                .entry(lineno - 1)
                .or_insert_with(|| (noqa::extract_noqa_directive(lines[lineno - 1]), vec![]));
        }
    }

    // Remove any ignored checks.
    for (index, check) in checks.iter().enumerate() {
        if check.kind == CheckKind::BlanketNOQA {
            continue;
        }

        // Is the check ignored by a `noqa` directive on the parent line?
        if let Some(parent_lineno) = check.parent.map(|location| location.row()) {
            let noqa_lineno = noqa_line_for.get(&parent_lineno).unwrap_or(&parent_lineno);
            if commented_lines.contains(noqa_lineno) {
                let noqa = noqa_directives.entry(noqa_lineno - 1).or_insert_with(|| {
                    (noqa::extract_noqa_directive(lines[noqa_lineno - 1]), vec![])
                });
                match noqa {
                    (Directive::All(..), matches) => {
                        matches.push(check.kind.code().as_ref());
                        ignored.push(index);
                        continue;
                    }
                    (Directive::Codes(.., codes), matches) => {
                        if noqa::includes(check.kind.code(), codes) {
                            matches.push(check.kind.code().as_ref());
                            ignored.push(index);
                            continue;
                        }
                    }
                    (Directive::None, ..) => {}
                }
            }
        }

        // Is the check ignored by a `noqa` directive on the same line?
        let check_lineno = check.location.row();
        let noqa_lineno = noqa_line_for.get(&check_lineno).unwrap_or(&check_lineno);
        if commented_lines.contains(noqa_lineno) {
            let noqa = noqa_directives
                .entry(noqa_lineno - 1)
                .or_insert_with(|| (noqa::extract_noqa_directive(lines[noqa_lineno - 1]), vec![]));
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
                            Range::new(Location::new(row + 1, start), Location::new(row + 1, end)),
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
                    let mut disabled_codes = vec![];
                    let mut unknown_codes = vec![];
                    let mut unmatched_codes = vec![];
                    let mut valid_codes = vec![];
                    let mut self_ignore = false;
                    for code in codes {
                        let code = CODE_REDIRECTS.get(code).map_or(code, AsRef::as_ref);
                        if code == CheckCode::RUF100.as_ref() {
                            self_ignore = true;
                            break;
                        }

                        if matches.contains(&code) || settings.external.contains(code) {
                            valid_codes.push(code);
                        } else {
                            if let Ok(check_code) = CheckCode::from_str(code) {
                                if settings.enabled.contains(&check_code) {
                                    unmatched_codes.push(code);
                                } else {
                                    disabled_codes.push(code);
                                }
                            } else {
                                unknown_codes.push(code);
                            }
                        }
                    }

                    if self_ignore {
                        continue;
                    }

                    if !(disabled_codes.is_empty()
                        && unknown_codes.is_empty()
                        && unmatched_codes.is_empty())
                    {
                        let mut check = Check::new(
                            CheckKind::UnusedNOQA(Some(UnusedCodes {
                                disabled: disabled_codes
                                    .iter()
                                    .map(|code| (*code).to_string())
                                    .collect(),
                                unknown: unknown_codes
                                    .iter()
                                    .map(|code| (*code).to_string())
                                    .collect(),
                                unmatched: unmatched_codes
                                    .iter()
                                    .map(|code| (*code).to_string())
                                    .collect(),
                            })),
                            Range::new(Location::new(row + 1, start), Location::new(row + 1, end)),
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
