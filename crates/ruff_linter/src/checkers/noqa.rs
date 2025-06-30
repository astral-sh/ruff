//! `NoQA` enforcement and validation.

use std::path::Path;

use itertools::Itertools;
use rustc_hash::FxHashSet;

use ruff_python_trivia::CommentRanges;
use ruff_text_size::Ranged;

use crate::fix::edits::delete_comment;
use crate::noqa::{
    Code, Directive, FileExemption, FileNoqaDirectives, NoqaDirectives, NoqaMapping,
};
use crate::registry::Rule;
use crate::rule_redirects::get_redirect_target;
use crate::rules::pygrep_hooks;
use crate::rules::ruff;
use crate::rules::ruff::rules::{UnusedCodes, UnusedNOQA};
use crate::settings::LinterSettings;
use crate::{Edit, Fix, Locator};

use super::ast::LintContext;

/// RUF100
pub(crate) fn check_noqa(
    context: &mut LintContext,
    path: &Path,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    noqa_line_for: &NoqaMapping,
    analyze_directives: bool,
    settings: &LinterSettings,
) -> Vec<usize> {
    // Identify any codes that are globally exempted (within the current file).
    let file_noqa_directives =
        FileNoqaDirectives::extract(locator, comment_ranges, &settings.external, path);

    // Extract all `noqa` directives.
    let mut noqa_directives =
        NoqaDirectives::from_commented_ranges(comment_ranges, &settings.external, path, locator);

    if file_noqa_directives.is_empty() && noqa_directives.is_empty() {
        return Vec::new();
    }

    let exemption = FileExemption::from(&file_noqa_directives);

    // Indices of diagnostics that were ignored by a `noqa` directive.
    let mut ignored_diagnostics = vec![];

    // Remove any ignored diagnostics.
    'outer: for (index, diagnostic) in context.iter().enumerate() {
        // Can't ignore syntax errors.
        let Some(code) = diagnostic.secondary_code() else {
            continue;
        };

        if *code == Rule::BlanketNOQA.noqa_code() {
            continue;
        }

        if exemption.contains_secondary_code(code) {
            ignored_diagnostics.push(index);
            continue;
        }

        let noqa_offsets = diagnostic
            .parent()
            .into_iter()
            .chain(std::iter::once(diagnostic.expect_range().start()))
            .map(|position| noqa_line_for.resolve(position))
            .unique();

        for noqa_offset in noqa_offsets {
            if let Some(directive_line) = noqa_directives.find_line_with_directive_mut(noqa_offset)
            {
                let suppressed = match &directive_line.directive {
                    Directive::All(_) => {
                        let Ok(rule) = Rule::from_code(code) else {
                            debug_assert!(false, "Invalid secondary code `{code}`");
                            continue;
                        };
                        directive_line.matches.push(rule);
                        ignored_diagnostics.push(index);
                        true
                    }
                    Directive::Codes(directive) => {
                        if directive.includes(code) {
                            let Ok(rule) = Rule::from_code(code) else {
                                debug_assert!(false, "Invalid secondary code `{code}`");
                                continue;
                            };
                            directive_line.matches.push(rule);
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
    if context.is_rule_enabled(Rule::UnusedNOQA)
        && analyze_directives
        && !exemption.includes(Rule::UnusedNOQA)
    {
        let directives = noqa_directives
            .lines()
            .iter()
            .map(|line| (&line.directive, &line.matches, false))
            .chain(
                file_noqa_directives
                    .lines()
                    .iter()
                    .map(|line| (&line.parsed_file_exemption, &line.matches, true)),
            );
        for (directive, matches, is_file_level) in directives {
            match directive {
                Directive::All(directive) => {
                    if matches.is_empty() {
                        let edit = delete_comment(directive.range(), locator);
                        let mut diagnostic = context
                            .report_diagnostic(UnusedNOQA { codes: None }, directive.range());
                        diagnostic.set_fix(Fix::safe_edit(edit));
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

                        if seen_codes.insert(original_code) {
                            let is_code_used = if is_file_level {
                                context.iter().any(|diag| {
                                    diag.secondary_code().is_some_and(|noqa| *noqa == code)
                                })
                            } else {
                                matches.iter().any(|match_| match_.noqa_code() == code)
                            } || settings
                                .external
                                .iter()
                                .any(|external| code.starts_with(external));

                            if is_code_used {
                                valid_codes.push(original_code);
                            } else if let Ok(rule) = Rule::from_code(code) {
                                if context.is_rule_enabled(rule) {
                                    unmatched_codes.push(original_code);
                                } else {
                                    disabled_codes.push(original_code);
                                }
                            } else {
                                unknown_codes.push(original_code);
                            }
                        } else {
                            duplicated_codes.push(original_code);
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
                            let original_text = locator.slice(directive.range());
                            let prefix = if is_file_level {
                                if original_text.contains("flake8") {
                                    "# flake8: noqa: "
                                } else {
                                    "# ruff: noqa: "
                                }
                            } else {
                                "# noqa: "
                            };
                            Edit::range_replacement(
                                format!("{}{}", prefix, valid_codes.join(", ")),
                                directive.range(),
                            )
                        };
                        let mut diagnostic = context.report_diagnostic(
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
                    }
                }
            }
        }
    }

    if context.is_rule_enabled(Rule::RedirectedNOQA) && !exemption.includes(Rule::RedirectedNOQA) {
        ruff::rules::redirected_noqa(context, &noqa_directives);
        ruff::rules::redirected_file_noqa(context, &file_noqa_directives);
    }

    if context.is_rule_enabled(Rule::BlanketNOQA) && !exemption.enumerates(Rule::BlanketNOQA) {
        pygrep_hooks::rules::blanket_noqa(
            context,
            &noqa_directives,
            locator,
            &file_noqa_directives,
        );
    }

    if context.is_rule_enabled(Rule::InvalidRuleCode)
        && !exemption.enumerates(Rule::InvalidRuleCode)
    {
        ruff::rules::invalid_noqa_code(context, &noqa_directives, locator, &settings.external);
    }

    ignored_diagnostics.sort_unstable();
    ignored_diagnostics
}
