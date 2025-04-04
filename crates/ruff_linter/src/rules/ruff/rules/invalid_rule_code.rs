use crate::noqa::{Code, Directive};
use crate::registry::Rule;
use crate::Locator;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::noqa::{Codes, NoqaDirectives};

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
pub(crate) struct InvalidRuleCode {
    pub(crate) rule_code: String,
}

impl AlwaysFixableViolation for InvalidRuleCode {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Invalid rule code in `# noqa`: {}", self.rule_code)
    }

    fn fix_title(&self) -> String {
        "Remove the rule code".to_string()
    }
}

/// RUF102 for invalid noqa codes
pub(crate) fn invalid_noqa_code(
    diagnostics: &mut Vec<Diagnostic>,
    noqa_directives: &NoqaDirectives,
    locator: &Locator,
    external: &[String],
) {
    for line in noqa_directives.lines() {
        let Directive::Codes(directive) = &line.directive else {
            continue;
        };

        let all_valid = directive.iter().all(|code| code_is_valid(code, external));

        if all_valid {
            continue;
        }

        let (valid_codes, invalid_codes): (Vec<_>, Vec<_>) = directive
            .iter()
            .partition(|&code| code_is_valid(code, external));

        if valid_codes.is_empty() {
            diagnostics.push(all_codes_invalid_diagnostic(directive, invalid_codes));
        } else {
            diagnostics.extend(invalid_codes.into_iter().map(|invalid_code| {
                some_codes_are_invalid_diagnostic(directive, invalid_code, locator)
            }));
        }
    }
}

fn code_is_valid(code: &Code, external: &[String]) -> bool {
    let code_str = code.as_str();
    Rule::from_code(code_str).is_ok() || external.iter().any(|ext| code_str.starts_with(ext))
}

fn all_codes_invalid_diagnostic(
    directive: &Codes<'_>,
    invalid_codes: Vec<&Code<'_>>,
) -> Diagnostic {
    Diagnostic::new(
        InvalidRuleCode {
            rule_code: invalid_codes
                .into_iter()
                .map(Code::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        },
        directive.range(),
    )
    .with_fix(Fix::safe_edit(Edit::range_deletion(directive.range())))
}

fn some_codes_are_invalid_diagnostic(
    codes: &Codes,
    invalid_code: &Code,
    locator: &Locator,
) -> Diagnostic {
    let diagnostic = Diagnostic::new(
        InvalidRuleCode {
            rule_code: invalid_code.to_string(),
        },
        invalid_code.range(),
    );
    diagnostic.with_fix(Fix::safe_edit(remove_invalid_noqa(
        codes,
        invalid_code,
        locator,
    )))
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
