//! `NoQA` enforcement and validation.

use std::path::Path;

use itertools::Itertools;
use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::Ranged;

use crate::fix::edits::delete_comment;
use crate::noqa::{
    Code, Directive, FileExemption, FileNoqaDirectives, NoqaDirectives, NoqaMapping,
};
use crate::registry::{AsRule, Rule, RuleSet};
use crate::rule_redirects::get_redirect_target;
use crate::rules::pygrep_hooks;
use crate::rules::ruff;
use crate::rules::ruff::rules::{UnusedCodes, UnusedNOQA};
use crate::settings::LinterSettings;
use crate::Locator;

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
    let file_noqa_directives =
        FileNoqaDirectives::extract(locator, comment_ranges, &settings.external, path);
    let exemption = FileExemption::from(&file_noqa_directives);

    // Extract all `noqa` directives.
    let mut noqa_directives =
        NoqaDirectives::from_commented_ranges(comment_ranges, &settings.external, path, locator);

    // Indices of diagnostics that were ignored by a `noqa` directive.
    let mut ignored_diagnostics = vec![];

    // Remove any ignored diagnostics.
    'outer: for (index, diagnostic) in diagnostics.iter().enumerate() {
        if matches!(diagnostic.kind.rule(), Rule::BlanketNOQA) {
            continue;
        }

        match &exemption {
            FileExemption::All(_) => {
                // If the file is exempted, ignore all diagnostics.
                ignored_diagnostics.push(index);
                continue;
            }
            FileExemption::Codes(codes) => {
                // If the diagnostic is ignored by a global exemption, ignore it.
                if codes.contains(&&diagnostic.kind.rule().noqa_code()) {
                    ignored_diagnostics.push(index);
                    continue;
                }
            }
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
                        if directive.includes(diagnostic.kind.rule()) {
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
        && !exemption.includes(Rule::UnusedNOQA)
        && !per_file_ignores.contains(Rule::UnusedNOQA)
    {
        for line in noqa_directives.lines() {
            match &line.directive {
                Directive::All(directive) => {
                    if line.matches.is_empty() {
                        let edit = delete_comment(directive.range(), locator);
                        let mut diagnostic =
                            Diagnostic::new(UnusedNOQA { codes: None }, directive.range());
                        diagnostic.set_fix(Fix::safe_edit(edit));

                        diagnostics.push(diagnostic);
                    }
                }
                Directive::Codes(directive) => {
                    let mut disabled_codes = vec![];
                    let mut duplicated_codes = vec![];
                    let mut unknown_codes = vec![];
                    let mut unmatched_codes = vec![];
                    let mut valid_codes = vec![];
                    let mut seen_codes = FxHashSet::default();
                    let mut self_ignore = false;
                    for original_code in directive.iter().map(Code::as_str) {
                        let code = get_redirect_target(original_code).unwrap_or(original_code);
                        if Rule::UnusedNOQA.noqa_code() == code {
                            self_ignore = true;
                            break;
                        }

                        if !seen_codes.insert(original_code) {
                            duplicated_codes.push(original_code);
                        } else if line.matches.iter().any(|match_| *match_ == code)
                            || settings
                                .external
                                .iter()
                                .any(|external| code.starts_with(external))
                        {
                            valid_codes.push(original_code);
                        } else {
                            if let Ok(rule) = Rule::from_code(code) {
                                if settings.rules.enabled(rule) {
                                    unmatched_codes.push(original_code);
                                } else {
                                    disabled_codes.push(original_code);
                                }
                            } else {
                                unknown_codes.push(original_code);
                            }
                        }
                    }

                    if self_ignore {
                        continue;
                    }

                    if !(disabled_codes.is_empty()
                        && duplicated_codes.is_empty()
                        && unknown_codes.is_empty()
                        && unmatched_codes.is_empty())
                    {
                        let edit = if valid_codes.is_empty() {
                            delete_comment(directive.range(), locator)
                        } else {
                            Edit::range_replacement(
                                format!("# noqa: {}", valid_codes.join(", ")),
                                directive.range(),
                            )
                        };
                        let mut diagnostic = Diagnostic::new(
                            UnusedNOQA {
                                codes: Some(UnusedCodes {
                                    disabled: disabled_codes
                                        .iter()
                                        .map(|code| (*code).to_string())
                                        .collect(),
                                    duplicated: duplicated_codes
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
                        diagnostic.set_fix(Fix::safe_edit(edit));
                        diagnostics.push(diagnostic);
                    }
                }
            }
        }
    }

    if settings.rules.enabled(Rule::RedirectedNOQA)
        && !per_file_ignores.contains(Rule::RedirectedNOQA)
        && !exemption.includes(Rule::RedirectedNOQA)
    {
        ruff::rules::redirected_noqa(diagnostics, &noqa_directives);
        ruff::rules::redirected_file_noqa(diagnostics, &file_noqa_directives);
    }

    if settings.rules.enabled(Rule::BlanketNOQA)
        && !per_file_ignores.contains(Rule::BlanketNOQA)
        && !exemption.enumerates(Rule::BlanketNOQA)
    {
        pygrep_hooks::rules::blanket_noqa(
            diagnostics,
            &noqa_directives,
            locator,
            &file_noqa_directives,
        );
    }

    if settings.rules.enabled(Rule::InvalidRuleCode)
        && !per_file_ignores.contains(Rule::InvalidRuleCode)
        && !exemption.enumerates(Rule::InvalidRuleCode)
    {
        ruff::rules::invalid_noqa_code(diagnostics, &noqa_directives, locator, &settings.external);
    }

    ignored_diagnostics.sort_unstable();
    ignored_diagnostics
}
