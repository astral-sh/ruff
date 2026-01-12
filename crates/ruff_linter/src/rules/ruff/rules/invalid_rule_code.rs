use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::noqa::{Code, Directive};
use crate::noqa::{Codes, NoqaDirectives};
use crate::registry::Rule;
use crate::rule_redirects::get_redirect_target;
use crate::{AlwaysFixableViolation, Edit, Fix};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InvalidRuleCodeKind {
    Noqa,
    Suppression,
}

impl InvalidRuleCodeKind {
    fn as_str(&self) -> &str {
        match self {
            InvalidRuleCodeKind::Noqa => "`# noqa`",
            InvalidRuleCodeKind::Suppression => "suppression",
        }
    }
}

/// ## What it does
/// Checks for `noqa` codes that are invalid.
///
/// ## Why is this bad?
/// Invalid rule codes serve no purpose and may indicate outdated code suppressions.
///
/// ## Example
/// ```python
/// import os  # noqa: XYZ999
/// ```
///
/// Use instead:
/// ```python
/// import os
/// ```
///
/// Or if there are still valid codes needed:
/// ```python
/// import os  # noqa: E402
/// ```
///
/// ## Options
/// - `lint.external`
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.11.4")]
pub(crate) struct InvalidRuleCode {
    pub(crate) rule_code: String,
    pub(crate) kind: InvalidRuleCodeKind,
    pub(crate) whole_comment: bool,
}

impl AlwaysFixableViolation for InvalidRuleCode {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Invalid rule code in {}: {}",
            self.kind.as_str(),
            self.rule_code
        )
    }

    fn fix_title(&self) -> String {
        if self.whole_comment {
            format!("Remove the {} comment", self.kind.as_str())
        } else {
            format!("Remove the rule code `{}`", self.rule_code)
        }
    }
}

/// RUF102 for invalid noqa codes
pub(crate) fn invalid_noqa_code(
    context: &LintContext,
    noqa_directives: &NoqaDirectives,
    locator: &Locator,
    external: &[String],
) {
    for line in noqa_directives.lines() {
        let Directive::Codes(directive) = &line.directive else {
            continue;
        };

        let all_valid = directive
            .iter()
            .all(|code| code_is_valid(code.as_str(), external));

        if all_valid {
            continue;
        }

        let (valid_codes, invalid_codes): (Vec<_>, Vec<_>) = directive
            .iter()
            .partition(|&code| code_is_valid(code.as_str(), external));

        if valid_codes.is_empty() {
            all_codes_invalid_diagnostic(directive, invalid_codes, context);
        } else {
            for invalid_code in invalid_codes {
                some_codes_are_invalid_diagnostic(directive, invalid_code, locator, context);
            }
        }
    }
}

pub(crate) fn code_is_valid(code: &str, external: &[String]) -> bool {
    Rule::from_code(get_redirect_target(code).unwrap_or(code)).is_ok()
        || external.iter().any(|ext| code.starts_with(ext))
}

fn all_codes_invalid_diagnostic(
    directive: &Codes<'_>,
    invalid_codes: Vec<&Code<'_>>,
    context: &LintContext,
) {
    context
        .report_diagnostic(
            InvalidRuleCode {
                rule_code: invalid_codes
                    .into_iter()
                    .map(Code::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
                kind: InvalidRuleCodeKind::Noqa,
                whole_comment: true,
            },
            directive.range(),
        )
        .set_fix(Fix::safe_edit(Edit::range_deletion(directive.range())));
}

fn some_codes_are_invalid_diagnostic(
    codes: &Codes,
    invalid_code: &Code,
    locator: &Locator,
    context: &LintContext,
) {
    context
        .report_diagnostic(
            InvalidRuleCode {
                rule_code: invalid_code.to_string(),
                kind: InvalidRuleCodeKind::Noqa,
                whole_comment: false,
            },
            invalid_code.range(),
        )
        .set_fix(Fix::safe_edit(remove_invalid_noqa(
            codes,
            invalid_code,
            locator,
        )));
}

fn remove_invalid_noqa(codes: &Codes, invalid_code: &Code, locator: &Locator) -> Edit {
    // Is this the first code after the `:` that needs to get deleted
    // For the first element, delete from after the `:` to the next comma (including)
    // For any other element, delete from the previous comma (including) to the next comma (excluding)
    let mut first = false;

    // Find the index of the previous comma or colon.
    let start = locator
        .slice(TextRange::new(codes.start(), invalid_code.start()))
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
        .unwrap_or(invalid_code.start());

    // Find the index of the trailing comma (if any)
    let end = locator
        .slice(TextRange::new(invalid_code.end(), codes.end()))
        .find(',')
        .map(|offset| {
            let offset = invalid_code.end() + TextSize::try_from(offset).unwrap();

            if first {
                offset + ','.text_len()
            } else {
                offset
            }
        })
        .unwrap_or(invalid_code.end());

    Edit::range_deletion(TextRange::new(start, end))
}
