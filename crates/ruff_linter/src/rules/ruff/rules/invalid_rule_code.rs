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

        let mut invalid_code_refs = vec![];
        let mut all_invalid = true;

        for code in directive.iter() {
            let code_str = code.as_str();
            if Rule::from_code(code_str).is_ok()
                || external.iter().any(|ext| code_str.starts_with(ext))
            {
                all_invalid = false;
            } else {
                invalid_code_refs.push(code);
            }
        }

        if invalid_code_refs.is_empty() {
            continue;
        }

        if all_invalid {
            handle_all_codes_invalid(diagnostics, directive);
        } else {
            let valid_codes = directive
                .iter()
                .filter_map(|code| {
                    let code_str = code.as_str();
                    if external.iter().any(|ext| code_str.starts_with(ext))
                        || Rule::from_code(code_str).is_ok()
                    {
                        Some(code_str)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            handle_some_codes_invalid(diagnostics, &invalid_code_refs, &valid_codes, line, locator);
        }
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
