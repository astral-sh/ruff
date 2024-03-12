//! `NoQA` enforcement and validation.

use std::path::Path;

use itertools::Itertools;
use ruff_text_size::Ranged;

use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::Locator;

use crate::fix::edits::delete_comment;
use crate::noqa;
use crate::noqa::{Directive, FileExemption, NoqaDirectives, NoqaMapping};
use crate::registry::{AsRule, Rule, RuleSet};
use crate::rule_redirects::get_redirect_target;
use crate::rules::ruff::rules::{UnusedCodes, UnusedNOQA};
use crate::settings::LinterSettings;

#[allow(clippy::too_many_arguments)]
pub(crate) fn check_noqa(
    diagnostics: &mut Vec<Diagnostic>,
    path: &Path,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    noqa_line_for: &NoqaMapping,
    analyze_directives: bool,
    per_file_ignores: &RuleSet,
    settings: &LinterSettings,
) -> Vec<usize> {
    // Identify any codes that are globally exempted (within the current file).
    let exemption = FileExemption::try_extract(
        locator.contents(),
        comment_ranges,
        &settings.external,
        path,
        locator,
    );

    // Extract all `noqa` directives.
    let mut noqa_directives = NoqaDirectives::from_commented_ranges(comment_ranges, path, locator);

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

    // Enforce that the noqa directive was actually used (RUF100), unless RUF100 was itself
    // suppressed.
    if settings.rules.enabled(Rule::UnusedNOQA)
        && analyze_directives
        && !exemption.is_some_and(|exemption| match exemption {
            FileExemption::All => true,
            FileExemption::Codes(codes) => codes.contains(&Rule::UnusedNOQA.noqa_code()),
        })
    {
        for line in noqa_directives.lines() {
            match &line.directive {
                Directive::All(directive) => {
                    if line.matches.is_empty() {
                        let mut diagnostic =
                            Diagnostic::new(UnusedNOQA { codes: None }, directive.range());
                        diagnostic
                            .set_fix(Fix::safe_edit(delete_comment(directive.range(), locator)));

                        diagnostics.push(diagnostic);
                    }
                }
                Directive::Codes(directive) => {
                    let mut disabled_codes = vec![];
                    let mut unknown_codes = vec![];
                    let mut unmatched_codes = vec![];
                    let mut valid_codes = vec![];
                    let mut self_ignore = per_file_ignores.contains(Rule::UnusedNOQA);
                    for code in directive.codes() {
                        let code = get_redirect_target(code).unwrap_or(code);
                        if Rule::UnusedNOQA.noqa_code() == code {
                            self_ignore = true;
                            break;
                        }

                        if line.matches.iter().any(|match_| *match_ == code)
                            || settings
                                .external
                                .iter()
                                .any(|external| code.starts_with(external))
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
                        if valid_codes.is_empty() {
                            diagnostic.set_fix(Fix::safe_edit(delete_comment(
                                directive.range(),
                                locator,
                            )));
                        } else {
                            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                                format!("# noqa: {}", valid_codes.join(", ")),
                                directive.range(),
                            )));
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
