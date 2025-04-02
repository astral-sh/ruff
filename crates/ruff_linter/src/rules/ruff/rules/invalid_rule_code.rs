use crate::noqa::{Code, Directive};
use crate::registry::Rule;
use crate::Locator;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::noqa::{Codes, NoqaDirectiveLine, NoqaDirectives};

/// ### What it does
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
) {
    for line in noqa_directives.lines() {
        let Directive::Codes(directive) = &line.directive else {
            continue;
        };

        let mut invalid_code_refs = vec![];
        let mut valid_codes = vec![];

        for code in directive.iter() {
            let code_str = code.as_str();
            if Rule::from_code(code.as_str()).is_err() {
                invalid_code_refs.push(code);
            } else {
                valid_codes.push(code_str);
            }
        }

        if invalid_code_refs.is_empty() {
            continue;
        }
        if valid_codes.is_empty() {
            handle_all_codes_invalid(diagnostics, directive);
            continue;
        }

        handle_some_codes_invalid(diagnostics, &invalid_code_refs, &valid_codes, line, locator);
    }
}

fn handle_all_codes_invalid(diagnostics: &mut Vec<Diagnostic>, directive: &Codes<'_>) {
    let invalid_codes = directive
        .iter()
        .map(crate::noqa::Code::as_str)
        .collect::<Vec<_>>()
        .join(", ");

    let mut diagnostic = Diagnostic::new(
        InvalidRuleCode {
            rule_code: invalid_codes,
        },
        directive.range(),
    );

    diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(directive.range())));
    diagnostics.push(diagnostic);
}

fn handle_some_codes_invalid(
    diagnostics: &mut Vec<Diagnostic>,
    invalid_codes: &[&Code<'_>],
    valid_codes: &[&str],
    line: &NoqaDirectiveLine<'_>,
    locator: &Locator,
) {
    let updated_noqa = update_noqa(line, valid_codes, locator);
    let fix = Fix::safe_edit(Edit::range_replacement(updated_noqa, line.range()));

    for invalid_code in invalid_codes {
        let mut diagnostic = Diagnostic::new(
            InvalidRuleCode {
                rule_code: invalid_code.as_str().to_string(),
            },
            invalid_code.range(),
        );

        diagnostic.set_fix(fix.clone());
        diagnostics.push(diagnostic);
    }
}

fn update_noqa(line: &NoqaDirectiveLine<'_>, valid_codes: &[&str], locator: &Locator) -> String {
    let noqa_slice = "noqa:";
    let formatted_codes = valid_codes.join(", ");
    let original_text = locator.slice(line.range());

    if let Some(noqa_idx) = original_text.find(noqa_slice) {
        let prefix_end = noqa_idx + noqa_slice.len();
        let prefix = &original_text[..prefix_end];
        let codes_part = &original_text[prefix_end..];
        let whitespace_len = codes_part.chars().take_while(|c| c.is_whitespace()).count();
        format!(
            "{}{}{}",
            prefix,
            &codes_part[..whitespace_len],
            formatted_codes
        )
    } else {
        format!("# noqa: {formatted_codes}")
    }
}
