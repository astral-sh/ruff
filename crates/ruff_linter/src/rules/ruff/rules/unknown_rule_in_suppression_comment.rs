use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::codes::Category;
use crate::fix::edits::delete_comment;
use crate::noqa::{Code, Directive, FileNoqaDirectives};
use crate::noqa::{Codes, NoqaDirectives};
use crate::preview::is_file_level_unknown_rule_enabled;
use crate::registry::Rule;
use crate::rule_redirects::get_redirect_target;
use crate::{AlwaysFixableViolation, Edit, Fix};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UnknownRuleCommentKind {
    Noqa,
    Suppression,
}

impl UnknownRuleCommentKind {
    fn as_str(&self) -> &str {
        match self {
            UnknownRuleCommentKind::Noqa => "`# noqa`",
            UnknownRuleCommentKind::Suppression => "suppression",
        }
    }
}

/// ## What it does
/// Checks for unknown rule names and codes in suppression comments.
///
/// ## Why is this bad?
/// Unknown rules in suppression comments serve no purpose and may indicate outdated suppressions.
///
/// ## Example
/// ```python
/// import os  # noqa: XYZ999
/// import sys  # ruff: ignore[unknown-rule]
/// ```
///
/// Use instead:
/// ```python
/// import os
/// import sys
/// ```
///
/// Or if there are still valid codes needed:
/// ```python
/// import os  # noqa: E402
/// import sys  # ruff: ignore[E402]
/// ```
///
/// ## Fix safety
///
/// The rule's fix is marked as unsafe when a full suppression comment would be removed and there
/// are other nested comments on the same line. Removing such a comment can change the behavior of
/// other suppression comments before or after the removed comment.
///
/// ## Options
///
/// This rule will flag rule codes that are unknown to Ruff, even if they are
/// valid for other tools. You can tell Ruff to ignore such codes by configuring
/// the list of known "external" rule codes with the following option:
///
/// - `lint.external`
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.15.0", category = Category::Suspicious)]
pub(crate) struct UnknownRuleInSuppressionComment {
    pub(crate) rule: String,
    pub(crate) kind: UnknownRuleCommentKind,
    pub(crate) whole_comment: bool,
}

impl AlwaysFixableViolation for UnknownRuleInSuppressionComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Unknown rule in {comment}: {rule}",
            comment = self.kind.as_str(),
            rule = self.rule,
        )
    }

    fn fix_title(&self) -> String {
        if self.whole_comment {
            format!("Remove the {} comment", self.kind.as_str())
        } else {
            format!("Remove the rule `{rule}`", rule = self.rule)
        }
    }
}

/// RUF102 for unknown noqa codes
pub(crate) fn unknown_rule_in_noqa(
    context: &LintContext,
    file_noqa_directives: &FileNoqaDirectives,
    noqa_directives: &NoqaDirectives,
    locator: &Locator,
    external: &[String],
) {
    let check_codes = |codes: &Codes<'_>| {
        let all_known = codes
            .iter()
            .all(|code| code_is_valid(code.as_str(), external));

        if all_known {
            return;
        }

        let (valid_codes, unknown_rules): (Vec<_>, Vec<_>) = codes
            .iter()
            .partition(|&code| code_is_valid(code.as_str(), external));

        if valid_codes.is_empty() {
            all_rules_unknown_diagnostic(codes, unknown_rules, locator, context);
        } else {
            for unknown_rule in unknown_rules {
                unknown_rule_diagnostic(codes, unknown_rule, locator, context);
            }
        }
    };

    if is_file_level_unknown_rule_enabled(context.settings()) {
        for line in file_noqa_directives.lines() {
            if let Directive::Codes(codes) = &line.parsed_file_exemption {
                check_codes(codes);
            }
        }
    }
    for line in noqa_directives.lines() {
        if let Directive::Codes(codes) = &line.directive {
            check_codes(codes);
        }
    }
}

pub(crate) fn code_is_valid(code: &str, external: &[String]) -> bool {
    Rule::from_code(get_redirect_target(code).unwrap_or(code)).is_ok()
        || external.iter().any(|ext| code.starts_with(ext))
}

fn all_rules_unknown_diagnostic(
    directive: &Codes<'_>,
    unknown_rules: Vec<&Code<'_>>,
    locator: &Locator,
    context: &LintContext,
) {
    let mut diagnostic = context.report_custom_diagnostic(
        UnknownRuleInSuppressionComment {
            rule: unknown_rules
                .into_iter()
                .map(Code::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            kind: UnknownRuleCommentKind::Noqa,
            whole_comment: true,
        },
        directive.range(),
    );
    diagnostic.set_fix(Fix::safe_edit(delete_comment(directive.range(), locator)));
    diagnostic.help("Add non-Ruff rule codes to the `lint.external` configuration option");
}

fn unknown_rule_diagnostic(
    codes: &Codes,
    unknown_rule: &Code,
    locator: &Locator,
    context: &LintContext,
) {
    let mut diagnostic = context.report_custom_diagnostic(
        UnknownRuleInSuppressionComment {
            rule: unknown_rule.to_string(),
            kind: UnknownRuleCommentKind::Noqa,
            whole_comment: false,
        },
        unknown_rule.range(),
    );
    diagnostic.set_fix(Fix::safe_edit(remove_unknown_rule_from_noqa(
        codes,
        unknown_rule,
        locator,
    )));
    diagnostic.help("Add non-Ruff rule codes to the `lint.external` configuration option");
}

fn remove_unknown_rule_from_noqa(codes: &Codes, unknown_rule: &Code, locator: &Locator) -> Edit {
    // Is this the first code after the `:` that needs to get deleted
    // For the first element, delete from after the `:` to the next comma (including)
    // For any other element, delete from the previous comma (including) to the next comma (excluding)
    let mut first = false;

    // Find the index of the previous comma or colon.
    let start = locator
        .slice(TextRange::new(codes.start(), unknown_rule.start()))
        .rmatch_indices([',', ':'])
        .next()
        .map(|(offset, text)| {
            let offset = codes.start() + TextSize::try_from(offset).unwrap();
            if text == ":" {
                first = true;
                // Don't include the colon in the deletion range, or the noqa comment becomes invalid
                offset + ':'.text_len()
            } else {
                offset
            }
        })
        .unwrap_or(unknown_rule.start());

    // Find the index of the trailing comma (if any)
    let end = locator
        .slice(TextRange::new(unknown_rule.end(), codes.end()))
        .find(',')
        .map(|offset| {
            let offset = unknown_rule.end() + TextSize::try_from(offset).unwrap();

            if first {
                offset + ','.text_len()
            } else {
                offset
            }
        })
        .unwrap_or(unknown_rule.end());

    Edit::range_deletion(TextRange::new(start, end))
}
