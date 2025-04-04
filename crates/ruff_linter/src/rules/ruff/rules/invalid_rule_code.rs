use crate::noqa::{Code, Directive};
use crate::registry::Rule;
use crate::Locator;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::noqa::{Codes, NoqaDirectiveLine, NoqaDirectives};

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
/// ## References
/// - [Ruff external codes](https://docs.astral.sh/ruff/settings/#lint_external)
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

        let (invalid_codes, valid_codes): (Vec<_>, Vec<_>) = directive
            .iter()
            .partition(|&code| !code_is_valid(code, external));

        if invalid_codes.is_empty() {
            continue;
        }

        match valid_codes.is_empty() {
            true => {
                let diagnostic = all_codes_invalid_diagnostic(directive, invalid_codes);
                diagnostics.push(diagnostic);
            }
            false => handle_some_codes_invalid(diagnostics, &invalid_codes, line, locator),
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
    let mut diagnostic = Diagnostic::new(
        InvalidRuleCode {
            rule_code: invalid_codes
                .into_iter()
                .map(Code::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        },
        directive.range(),
    );

    diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(directive.range())));
    diagnostic
}

fn handle_some_codes_invalid(
    diagnostics: &mut Vec<Diagnostic>,
    invalid_codes: &[&Code],
    line: &NoqaDirectiveLine<'_>,
    locator: &Locator,
) {
    let Directive::Codes(directive) = &line.directive else {
        return;
    };

    for &invalid_code in invalid_codes {
        let this_invalid_str = invalid_code.as_str();
        let codes_to_keep = directive
            .iter()
            .filter_map(|code| {
                let code_str = code.as_str();
                match code_str != this_invalid_str {
                    true => Some(code_str),
                    false => None,
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        let updated_noqa = update_noqa(line, &codes_to_keep, locator);
        let fix = Fix::safe_edit(Edit::range_replacement(updated_noqa, line.range()));

        let mut diagnostic = Diagnostic::new(
            InvalidRuleCode {
                rule_code: this_invalid_str.to_string(),
            },
            invalid_code.range(),
        );

        diagnostic.set_fix(fix);
        diagnostics.push(diagnostic);
    }
}

fn update_noqa(line: &NoqaDirectiveLine<'_>, formatted_codes: &str, locator: &Locator) -> String {
    let noqa_slice = "noqa:";
    let original_text = locator.slice(line.range());

    if let Some(noqa_idx) = original_text.find(noqa_slice) {
        let prefix_end = noqa_idx + noqa_slice.len();
        let (prefix, codes_part) = original_text.split_at(prefix_end);
        let whitespace_end = codes_part
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(codes_part.len());
        format!(
            "{}{}{}",
            prefix,
            &codes_part[..whitespace_end],
            formatted_codes
        )
    } else {
        format!("# noqa: {formatted_codes}")
    }
}
