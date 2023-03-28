//! `NoQA` enforcement and validation.

use nohash_hasher::IntMap;
use rustpython_parser::ast::Location;

use ruff_diagnostics::{Diagnostic, Edit};
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::types::Range;

use crate::codes::NoqaCode;
use crate::noqa;
use crate::noqa::{Directive, FileExemption};
use crate::registry::{AsRule, Rule};
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
) -> Vec<usize> {
    let enforce_noqa = settings.rules.enabled(Rule::UnusedNOQA);

    let lines: Vec<&str> = contents.universal_newlines().collect();

    // Identify any codes that are globally exempted (within the current file).
    let exemption = noqa::file_exemption(&lines, commented_lines);

    // Map from line number to `noqa` directive on that line, along with any codes
    // that were matched by the directive.
    let mut noqa_directives: IntMap<usize, (Directive, Vec<NoqaCode>)> = IntMap::default();

    // Extract all `noqa` directives.
    if enforce_noqa {
        for lineno in commented_lines {
            noqa_directives
                .entry(lineno - 1)
                .or_insert_with(|| (noqa::extract_noqa_directive(lines[lineno - 1]), vec![]));
        }
    }

    // Indices of diagnostics that were ignored by a `noqa` directive.
    let mut ignored_diagnostics = vec![];

    // Remove any ignored diagnostics.
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if matches!(diagnostic.kind.rule(), Rule::BlanketNOQA) {
            continue;
        }

        match &exemption {
            FileExemption::All => {
                // If the file is exempted, ignore all diagnostics.
                ignored_diagnostics.push(index);
                continue;
            }
            FileExemption::Codes(codes) => {
                // If the diagnostic is ignored by a global exemption, ignore it.
                if codes.contains(&diagnostic.kind.rule().noqa_code()) {
                    ignored_diagnostics.push(index);
                    continue;
                }
            }
            FileExemption::None => {}
        }

        let diagnostic_lineno = diagnostic.location.row();

        // Is the violation ignored by a `noqa` directive on the parent line?
        if let Some(parent_lineno) = diagnostic.parent.map(|location| location.row()) {
            if parent_lineno != diagnostic_lineno {
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
                        (Directive::Codes(.., codes, _), matches) => {
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
        }

        // Is the diagnostic ignored by a `noqa` directive on the same line?
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
                (Directive::Codes(.., codes, _), matches) => {
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
                Directive::All(leading_spaces, start_byte, end_byte, trailing_spaces) => {
                    if matches.is_empty() {
                        let start_char = lines[row][..start_byte].chars().count();
                        let end_char =
                            start_char + lines[row][start_byte..end_byte].chars().count();

                        let mut diagnostic = Diagnostic::new(
                            UnusedNOQA { codes: None },
                            Range::new(
                                Location::new(row + 1, start_char),
                                Location::new(row + 1, end_char),
                            ),
                        );
                        if autofix.into() && settings.rules.should_fix(diagnostic.kind.rule()) {
                            diagnostic.set_fix(delete_noqa(
                                row,
                                lines[row],
                                leading_spaces,
                                start_byte,
                                end_byte,
                                trailing_spaces,
                            ));
                        }
                        diagnostics.push(diagnostic);
                    }
                }
                Directive::Codes(leading_spaces, start_byte, end_byte, codes, trailing_spaces) => {
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
                                if settings.rules.enabled(rule) {
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
                        let start_char = lines[row][..start_byte].chars().count();
                        let end_char =
                            start_char + lines[row][start_byte..end_byte].chars().count();

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
                            Range::new(
                                Location::new(row + 1, start_char),
                                Location::new(row + 1, end_char),
                            ),
                        );
                        if autofix.into() && settings.rules.should_fix(diagnostic.kind.rule()) {
                            if valid_codes.is_empty() {
                                diagnostic.set_fix(delete_noqa(
                                    row,
                                    lines[row],
                                    leading_spaces,
                                    start_byte,
                                    end_byte,
                                    trailing_spaces,
                                ));
                            } else {
                                diagnostic.set_fix(Edit::replacement(
                                    format!("# noqa: {}", valid_codes.join(", ")),
                                    Location::new(row + 1, start_char),
                                    Location::new(row + 1, end_char),
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
    ignored_diagnostics
}

/// Generate a [`Edit`] to delete a `noqa` directive.
fn delete_noqa(
    row: usize,
    line: &str,
    leading_spaces: usize,
    start_byte: usize,
    end_byte: usize,
    trailing_spaces: usize,
) -> Edit {
    if start_byte - leading_spaces == 0 && end_byte == line.len() {
        // Ex) `# noqa`
        Edit::deletion(Location::new(row + 1, 0), Location::new(row + 2, 0))
    } else if end_byte == line.len() {
        // Ex) `x = 1  # noqa`
        let start_char = line[..start_byte].chars().count();
        let end_char = start_char + line[start_byte..end_byte].chars().count();
        Edit::deletion(
            Location::new(row + 1, start_char - leading_spaces),
            Location::new(row + 1, end_char + trailing_spaces),
        )
    } else if line[end_byte..].trim_start().starts_with('#') {
        // Ex) `x = 1  # noqa  # type: ignore`
        let start_char = line[..start_byte].chars().count();
        let end_char = start_char + line[start_byte..end_byte].chars().count();
        Edit::deletion(
            Location::new(row + 1, start_char),
            Location::new(row + 1, end_char + trailing_spaces),
        )
    } else {
        // Ex) `x = 1  # noqa here`
        let start_char = line[..start_byte].chars().count();
        let end_char = start_char + line[start_byte..end_byte].chars().count();
        Edit::deletion(
            Location::new(row + 1, start_char + 1 + 1),
            Location::new(row + 1, end_char + trailing_spaces),
        )
    }
}
