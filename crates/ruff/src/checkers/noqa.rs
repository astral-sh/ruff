//! `NoQA` enforcement and validation.

use itertools::Itertools;
use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_python_ast::source_code::Locator;

use crate::noqa;
use crate::noqa::{Directive, FileExemption, NoqaDirectives, NoqaMapping};
use crate::registry::{AsRule, Rule};
use crate::rule_redirects::get_redirect_target;
use crate::rules::ruff::rules::{UnusedCodes, UnusedNOQA};
use crate::settings::Settings;

pub(crate) fn check_noqa(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    comment_ranges: &[TextRange],
    noqa_line_for: &NoqaMapping,
    analyze_directives: bool,
    settings: &Settings,
) -> Vec<usize> {
    // Identify any codes that are globally exempted (within the current file).
    let exemption = noqa::file_exemption(locator.contents(), comment_ranges);

    // Extract all `noqa` directives.
    let mut noqa_directives = NoqaDirectives::from_commented_ranges(comment_ranges, locator);

    // Indices of diagnostics that were ignored by a `noqa` directive.
    let mut ignored_diagnostics = vec![];

    // Remove any ignored diagnostics.
    'outer: for (index, diagnostic) in diagnostics.iter().enumerate() {
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

        let noqa_offsets = diagnostic
            .parent
            .into_iter()
            .chain(std::iter::once(diagnostic.start()))
            .map(|position| noqa_line_for.resolve(position))
            .unique();

        for noqa_offset in noqa_offsets {
            if let Some(directive_line) = noqa_directives.find_line_with_directive_mut(noqa_offset)
            {
                let suppressed = match &directive_line.directive {
                    Directive::All(..) => {
                        directive_line
                            .matches
                            .push(diagnostic.kind.rule().noqa_code());
                        ignored_diagnostics.push(index);
                        true
                    }
                    Directive::Codes(.., codes, _) => {
                        if noqa::includes(diagnostic.kind.rule(), codes) {
                            directive_line
                                .matches
                                .push(diagnostic.kind.rule().noqa_code());
                            ignored_diagnostics.push(index);
                            true
                        } else {
                            false
                        }
                    }
                    Directive::None => unreachable!(),
                };

                if suppressed {
                    continue 'outer;
                }
            }
        }
    }

    // Enforce that the noqa directive was actually used (RUF100).
    if analyze_directives && settings.rules.enabled(Rule::UnusedNOQA) {
        for line in noqa_directives.lines() {
            match &line.directive {
                Directive::All(leading_spaces, noqa_range, trailing_spaces) => {
                    if line.matches.is_empty() {
                        let mut diagnostic =
                            Diagnostic::new(UnusedNOQA { codes: None }, *noqa_range);
                        if settings.rules.should_fix(diagnostic.kind.rule()) {
                            #[allow(deprecated)]
                            diagnostic.set_fix_from_edit(delete_noqa(
                                *leading_spaces,
                                *noqa_range,
                                *trailing_spaces,
                                locator,
                            ));
                        }
                        diagnostics.push(diagnostic);
                    }
                }
                Directive::Codes(leading_spaces, range, codes, trailing_spaces) => {
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

                        if line.matches.iter().any(|m| *m == code)
                            || settings.external.contains(code)
                        {
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
                            *range,
                        );
                        if settings.rules.should_fix(diagnostic.kind.rule()) {
                            if valid_codes.is_empty() {
                                #[allow(deprecated)]
                                diagnostic.set_fix_from_edit(delete_noqa(
                                    *leading_spaces,
                                    *range,
                                    *trailing_spaces,
                                    locator,
                                ));
                            } else {
                                #[allow(deprecated)]
                                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                                    format!("# noqa: {}", valid_codes.join(", ")),
                                    *range,
                                )));
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
    leading_spaces: TextSize,
    noqa_range: TextRange,
    trailing_spaces: TextSize,
    locator: &Locator,
) -> Edit {
    let line_range = locator.line_range(noqa_range.start());

    // Ex) `# noqa`
    if line_range
        == TextRange::new(
            noqa_range.start() - leading_spaces,
            noqa_range.end() + trailing_spaces,
        )
    {
        let full_line_end = locator.full_line_end(line_range.end());
        Edit::deletion(line_range.start(), full_line_end)
    }
    // Ex) `x = 1  # noqa`
    else if noqa_range.end() + trailing_spaces == line_range.end() {
        Edit::deletion(noqa_range.start() - leading_spaces, line_range.end())
    }
    // Ex) `x = 1  # noqa  # type: ignore`
    else if locator.contents()[usize::from(noqa_range.end() + trailing_spaces)..].starts_with('#')
    {
        Edit::deletion(noqa_range.start(), noqa_range.end() + trailing_spaces)
    }
    // Ex) `x = 1  # noqa here`
    else {
        Edit::deletion(
            noqa_range.start() + "# ".text_len(),
            noqa_range.end() + trailing_spaces,
        )
    }
}
