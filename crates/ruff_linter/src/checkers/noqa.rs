//! `NoQA` enforcement and validation.

use std::path::Path;

use itertools::Itertools;
use rustc_hash::FxHashSet;

use ruff_python_trivia::CommentRanges;
use ruff_text_size::{Ranged, TextRange};

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
use crate::suppression::Suppressions;
use crate::{Edit, Fix, Locator};

use super::ast::LintContext;

/// RUF100
#[expect(clippy::too_many_arguments)]
pub(crate) fn check_noqa(
    context: &mut LintContext,
    path: &Path,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    noqa_line_for: &NoqaMapping,
    analyze_directives: bool,
    settings: &LinterSettings,
    suppressions: &Suppressions,
) -> Vec<usize> {
    // Identify any codes that are globally exempted (within the current file).
    let file_noqa_directives =
        FileNoqaDirectives::extract(locator, comment_ranges, &settings.external, path);

    // Extract all `noqa` directives.
    let mut noqa_directives = NoqaDirectives::from_commented_ranges(comment_ranges, path, locator);

    if file_noqa_directives.is_empty() && noqa_directives.is_empty() && suppressions.is_empty() {
        return Vec::new();
    }

    let exemption = FileExemption::from(&file_noqa_directives);

    // Generate diagnostics for suppression comments before applying suppressions so that the
    // diagnostics can themselves be suppressed.
    suppressions.check_rule_codes(context, locator);

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

        // Apply file-level suppressions first
        if exemption.contains_secondary_code(code) {
            ignored_diagnostics.push(index);
            continue;
        }

        // Apply ranged suppressions next
        if suppressions.check_diagnostic(diagnostic) {
            ignored_diagnostics.push(index);
            continue;
        }

        // Apply end-of-line noqa suppressions last
        let noqa_offsets = diagnostic
            .parent()
            .into_iter()
            .chain(diagnostic.range().map(TextRange::start))
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

    // Only migrate directives that don't require RUF100 cleanup first.
    let check_unused_noqa = context.is_rule_enabled(Rule::UnusedNOQA)
        && analyze_directives
        && !exemption.includes(Rule::UnusedNOQA);
    let check_noqa_comment =
        context.is_rule_enabled(Rule::NoqaComments) && !exemption.enumerates(Rule::NoqaComments);

    if check_unused_noqa || check_noqa_comment {
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
                Directive::All(all) => {
                    if check_unused_noqa && matches.is_empty() {
                        let edit = delete_comment(all.range(), locator);
                        let mut diagnostic = context.report_diagnostic(
                            UnusedNOQA {
                                codes: None,
                                kind: ruff::rules::UnusedNOQAKind::Noqa,
                            },
                            all.range(),
                        );
                        diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Unnecessary);
                        diagnostic.set_fix(Fix::safe_edit(edit));
                    } else if check_noqa_comment {
                        ruff::rules::noqa_comments(
                            context,
                            locator,
                            is_file_level,
                            matches.is_empty(),
                            directive,
                            matches,
                            suppressions,
                        );
                    }
                }
                Directive::Codes(codes) => {
                    let mut disabled_codes = vec![];
                    let mut duplicated_codes = vec![];
                    let mut unmatched_codes = vec![];
                    let mut valid_codes = vec![];
                    let mut seen_codes = FxHashSet::default();
                    let mut self_ignore = false;
                    let mut suppress_noqa_comment = false;
                    for original_code in codes.iter().map(Code::as_str) {
                        let code = get_redirect_target(original_code).unwrap_or(original_code);
                        if seen_codes.insert(original_code) {
                            if Rule::UnusedNOQA.noqa_code() == code {
                                self_ignore = true;
                                if context.is_rule_enabled(Rule::UnusedNOQA) {
                                    valid_codes.push(original_code);
                                } else {
                                    disabled_codes.push(original_code);
                                }
                                continue;
                            }

                            if context.is_rule_enabled(Rule::NoqaComments)
                                && Rule::NoqaComments.noqa_code() == code
                            {
                                suppress_noqa_comment = true;
                                valid_codes.push(original_code);
                                continue;
                            }

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
                            }
                        } else {
                            duplicated_codes.push(original_code);
                        }
                    }

                    let has_unused_codes = !(disabled_codes.is_empty()
                        && duplicated_codes.is_empty()
                        && unmatched_codes.is_empty());

                    if check_unused_noqa && !self_ignore && has_unused_codes {
                        let edit = if valid_codes.is_empty() {
                            delete_comment(codes.range(), locator)
                        } else {
                            let original_text = locator.slice(codes.range());
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
                                codes.range(),
                            )
                        };
                        let mut diagnostic = context.report_diagnostic(
                            UnusedNOQA {
                                codes: Some(UnusedCodes {
                                    disabled: &disabled_codes,
                                    duplicated: &duplicated_codes,
                                    unmatched: &unmatched_codes,
                                }),
                                kind: ruff::rules::UnusedNOQAKind::Noqa,
                            },
                            codes.range(),
                        );
                        diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Unnecessary);
                        diagnostic.set_fix(Fix::safe_edit(edit));
                    } else if check_noqa_comment && !suppress_noqa_comment {
                        ruff::rules::noqa_comments(
                            context,
                            locator,
                            is_file_level,
                            has_unused_codes,
                            directive,
                            matches,
                            suppressions,
                        );
                    }
                }
            }
        }
    }

    // Diagnostics for unused/invalid range suppressions
    suppressions.check_suppressions(context, locator);

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
        ruff::rules::invalid_noqa_code(
            context,
            &file_noqa_directives,
            &noqa_directives,
            locator,
            &settings.external,
        );
    }

    ignored_diagnostics.sort_unstable();
    ignored_diagnostics
}
