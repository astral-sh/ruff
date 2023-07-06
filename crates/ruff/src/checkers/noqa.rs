//! `NoQA` enforcement and validation.

use itertools::Itertools;
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_parser::ast::Ranged;

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
    let exemption = FileExemption::try_extract(locator.contents(), comment_ranges, locator);

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
            Some(FileExemption::All) => {
                // If the file is exempted, ignore all diagnostics.
                ignored_diagnostics.push(index);
                continue;
            }
            Some(FileExemption::Codes(codes)) => {
                // If the diagnostic is ignored by a global exemption, ignore it.
                if codes.contains(&diagnostic.kind.rule().noqa_code()) {
                    ignored_diagnostics.push(index);
                    continue;
                }
            }
            None => {}
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
                    Directive::All(_) => {
                        directive_line
                            .matches
                            .push(diagnostic.kind.rule().noqa_code());
                        ignored_diagnostics.push(index);
                        true
                    }
                    Directive::Codes(directive) => {
                        if noqa::includes(diagnostic.kind.rule(), directive.codes()) {
                            directive_line
                                .matches
                                .push(diagnostic.kind.rule().noqa_code());
                            ignored_diagnostics.push(index);
                            true
                        } else {
                            false
                        }
                    }
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
                Directive::All(directive) => {
                    if line.matches.is_empty() {
                        let mut diagnostic =
                            Diagnostic::new(UnusedNOQA { codes: None }, directive.range());
                        if settings.rules.should_fix(diagnostic.kind.rule()) {
                            #[allow(deprecated)]
                            diagnostic.set_fix_from_edit(delete_noqa(directive.range(), locator));
                        }
                        diagnostics.push(diagnostic);
                    }
                }
                Directive::Codes(directive) => {
                    let mut disabled_codes = vec![];
                    let mut unknown_codes = vec![];
                    let mut unmatched_codes = vec![];
                    let mut valid_codes = vec![];
                    let mut self_ignore = false;
                    for code in directive.codes() {
                        let code = get_redirect_target(code).unwrap_or(code);
                        if Rule::UnusedNOQA.noqa_code() == code {
                            self_ignore = true;
                            break;
                        }

                        if line.matches.iter().any(|match_| *match_ == code)
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
                            directive.range(),
                        );
                        if settings.rules.should_fix(diagnostic.kind.rule()) {
                            if valid_codes.is_empty() {
                                #[allow(deprecated)]
                                diagnostic
                                    .set_fix_from_edit(delete_noqa(directive.range(), locator));
                            } else {
                                #[allow(deprecated)]
                                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                                    format!("# noqa: {}", valid_codes.join(", ")),
                                    directive.range(),
                                )));
                            }
                        }
                        diagnostics.push(diagnostic);
                    }
                }
            }
        }
    }

    ignored_diagnostics.sort_unstable();
    ignored_diagnostics
}

/// Generate a [`Edit`] to delete a `noqa` directive.
fn delete_noqa(range: TextRange, locator: &Locator) -> Edit {
    let line_range = locator.line_range(range.start());

    // Compute the leading space.
    let prefix = locator.slice(TextRange::new(line_range.start(), range.start()));
    let leading_space = prefix
        .rfind(|c: char| !c.is_whitespace())
        .map_or(prefix.len(), |i| prefix.len() - i - 1);
    let leading_space_len = TextSize::try_from(leading_space).unwrap();

    // Compute the trailing space.
    let suffix = locator.slice(TextRange::new(range.end(), line_range.end()));
    let trailing_space = suffix
        .find(|c: char| !c.is_whitespace())
        .map_or(suffix.len(), |i| i);
    let trailing_space_len = TextSize::try_from(trailing_space).unwrap();

    // Ex) `# noqa`
    if line_range
        == TextRange::new(
            range.start() - leading_space_len,
            range.end() + trailing_space_len,
        )
    {
        let full_line_end = locator.full_line_end(line_range.end());
        Edit::deletion(line_range.start(), full_line_end)
    }
    // Ex) `x = 1  # noqa`
    else if range.end() + trailing_space_len == line_range.end() {
        Edit::deletion(range.start() - leading_space_len, line_range.end())
    }
    // Ex) `x = 1  # noqa  # type: ignore`
    else if locator.contents()[usize::from(range.end() + trailing_space_len)..].starts_with('#') {
        Edit::deletion(range.start(), range.end() + trailing_space_len)
    }
    // Ex) `x = 1  # noqa here`
    else {
        Edit::deletion(
            range.start() + "# ".text_len(),
            range.end() + trailing_space_len,
        )
    }
}
