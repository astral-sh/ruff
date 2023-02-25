//! `NoQA` enforcement and validation.

use log::warn;
use nohash_hasher::IntMap;
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::codes::NoqaCode;
use crate::fix::Fix;
use crate::noqa;
use crate::noqa::{extract_file_exemption, Directive, Exemption};
use crate::registry::{Diagnostic, DiagnosticKind, Rule};
use crate::rule_redirects::get_redirect_target;
use crate::rules::ruff::rules::{UnusedCodes, UnusedNOQA};
use crate::settings::{flags, Settings};

pub fn check_noqa(
    diagnostics: &mut Vec<Diagnostic>,
    contents: &str,
    commented_lines: &[usize],
    noqa_line_for: &IntMap<usize, usize>,
    settings: &Settings,
    autofix: flags::Autofix,
) {
    let enforce_noqa = settings.rules.enabled(&Rule::UnusedNOQA);

    // Whether the file is exempted from all checks.
    let mut file_exempted = false;

    // Codes that are globally exempted (within the current file).
    let mut file_exemptions: Vec<NoqaCode> = vec![];

    // Map from line number to `noqa` directive on that line, along with any codes
    // that were matched by the directive.
    let mut noqa_directives: IntMap<usize, (Directive, Vec<NoqaCode>)> = IntMap::default();

    // Indices of diagnostics that were ignored by a `noqa` directive.
    let mut ignored_diagnostics = vec![];

    let lines: Vec<&str> = contents.lines().collect();
    for lineno in commented_lines {
        match extract_file_exemption(lines[lineno - 1]) {
            Exemption::All => {
                file_exempted = true;
            }
            Exemption::Codes(codes) => {
                file_exemptions.extend(codes.into_iter().filter_map(|code| {
                    if let Ok(rule) = Rule::from_code(get_redirect_target(code).unwrap_or(code)) {
                        Some(rule.noqa_code())
                    } else {
                        warn!("Invalid code provided to `# ruff: noqa`: {}", code);
                        None
                    }
                }));
            }
            Exemption::None => {}
        }

        if enforce_noqa {
            noqa_directives
                .entry(lineno - 1)
                .or_insert_with(|| (noqa::extract_noqa_directive(lines[lineno - 1]), vec![]));
        }
    }

    // Remove any ignored diagnostics.
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if matches!(diagnostic.kind, DiagnosticKind::BlanketNOQA(..)) {
            continue;
        }

        // If the file is exempted, ignore all diagnostics.
        if file_exempted {
            ignored_diagnostics.push(index);
            continue;
        }

        // If the diagnostic is ignored by a global exemption, ignore it.
        if !file_exemptions.is_empty() {
            if file_exemptions.contains(&diagnostic.kind.rule().noqa_code()) {
                ignored_diagnostics.push(index);
                continue;
            }
        }

        // Is the violation ignored by a `noqa` directive on the parent line?
        if let Some(parent_lineno) = diagnostic.parent.map(|location| location.row()) {
            let noqa_lineno = noqa_line_for.get(&parent_lineno).unwrap_or(&parent_lineno);
            if commented_lines.contains(noqa_lineno) {
                let noqa = noqa_directives.entry(noqa_lineno - 1).or_insert_with(|| {
                    (noqa::extract_noqa_directive(lines[noqa_lineno - 1]), vec![])
                });
                match noqa {
                    (Directive::All(..), matches) => {
                        matches.push(diagnostic.kind.rule().noqa_code());
                        ignored_diagnostics.push(index);
                        continue;
                    }
                    (Directive::Codes(.., codes), matches) => {
                        if noqa::includes(diagnostic.kind.rule(), codes) {
                            matches.push(diagnostic.kind.rule().noqa_code());
                            ignored_diagnostics.push(index);
                            continue;
                        }
                    }
                    (Directive::None, ..) => {}
                }
            }
        }

        // Is the diagnostic ignored by a `noqa` directive on the same line?
        let diagnostic_lineno = diagnostic.location.row();
        let noqa_lineno = noqa_line_for
            .get(&diagnostic_lineno)
            .unwrap_or(&diagnostic_lineno);
        if commented_lines.contains(noqa_lineno) {
            let noqa = noqa_directives
                .entry(noqa_lineno - 1)
                .or_insert_with(|| (noqa::extract_noqa_directive(lines[noqa_lineno - 1]), vec![]));
            match noqa {
                (Directive::All(..), matches) => {
                    matches.push(diagnostic.kind.rule().noqa_code());
                    ignored_diagnostics.push(index);
                    continue;
                }
                (Directive::Codes(.., codes), matches) => {
                    if noqa::includes(diagnostic.kind.rule(), codes) {
                        matches.push(diagnostic.kind.rule().noqa_code());
                        ignored_diagnostics.push(index);
                        continue;
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
                Directive::All(spaces, start_byte, end_byte) => {
                    if matches.is_empty() {
                        let start = lines[row][..start_byte].chars().count();
                        let end = start + lines[row][start_byte..end_byte].chars().count();

                        let mut diagnostic = Diagnostic::new(
                            UnusedNOQA { codes: None },
                            Range::new(Location::new(row + 1, start), Location::new(row + 1, end)),
                        );
                        if autofix.into() && settings.rules.should_fix(diagnostic.kind.rule()) {
                            diagnostic.amend(Fix::deletion(
                                Location::new(row + 1, start - spaces),
                                Location::new(row + 1, lines[row].chars().count()),
                            ));
                        }
                        diagnostics.push(diagnostic);
                    }
                }
                Directive::Codes(spaces, start_byte, end_byte, codes) => {
                    let mut disabled_codes = vec![];
                    let mut unknown_codes = vec![];
                    let mut unmatched_codes = vec![];
                    let mut valid_codes = vec![];
                    let mut self_ignore = false;
                    for code in codes {
                        let code = get_redirect_target(code).unwrap_or(code);
                        if Rule::UnusedNOQA.noqa_code() == code {
                            self_ignore = true;
                            break;
                        }

                        if matches.iter().any(|m| *m == code) || settings.external.contains(code) {
                            valid_codes.push(code);
                        } else {
                            if let Ok(rule) = Rule::from_code(code) {
                                if settings.rules.enabled(&rule) {
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
                        let start = lines[row][..start_byte].chars().count();
                        let end = start + lines[row][start_byte..end_byte].chars().count();

                        let mut diagnostic = Diagnostic::new(
                            UnusedNOQA {
                                codes: Some(UnusedCodes {
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
                                }),
                            },
                            Range::new(Location::new(row + 1, start), Location::new(row + 1, end)),
                        );
                        if autofix.into() && settings.rules.should_fix(diagnostic.kind.rule()) {
                            if valid_codes.is_empty() {
                                diagnostic.amend(Fix::deletion(
                                    Location::new(row + 1, start - spaces),
                                    Location::new(row + 1, lines[row].chars().count()),
                                ));
                            } else {
                                diagnostic.amend(Fix::replacement(
                                    format!("# noqa: {}", valid_codes.join(", ")),
                                    Location::new(row + 1, start),
                                    Location::new(row + 1, lines[row].chars().count()),
                                ));
                            }
                        }
                        diagnostics.push(diagnostic);
                    }
                }
                Directive::None => {}
            }
        }
    }

    ignored_diagnostics.sort_unstable();
    for index in ignored_diagnostics.iter().rev() {
        diagnostics.swap_remove(*index);
    }
}
