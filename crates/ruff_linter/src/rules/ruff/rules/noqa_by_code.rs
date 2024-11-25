use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::{
    noqa::{Directive, NoqaDirectives, NoqaIdentifier},
    registry::Rule,
    rule_redirects::get_redirect_target,
};

/// ## What it does
/// Checks for uses of rule codes in noqa comments.
///
/// ## Why is this bad?
/// Rule names are more readable and convey meaning better than codes.
/// However, codes are still provided for backwards compatibility with other tools.
///
/// ## Example
/// ```python
/// from typing import Never  # noqa: F401
/// ```
///
/// Use instead:
/// ```python
/// from typing import Never  # noqa: unused-import
/// ```
#[violation]
pub struct NOQAByCode {
    pub codes_and_names: Vec<(String, String)>,
}

impl AlwaysFixableViolation for NOQAByCode {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NOQAByCode { codes_and_names } = self;
        format!(
            "Noqa directive lists rule codes instead of rule names: {}",
            codes_and_names
                .iter()
                .map(|(code, name)| format!("{code} ({name})"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    fn fix_title(&self) -> String {
        "Use rule names in noqa directive".to_string()
    }
}

/// RUF102
pub(crate) fn noqa_by_code(diagnostics: &mut Vec<Diagnostic>, noqa_directives: &NoqaDirectives) {
    'line: for line in noqa_directives.lines() {
        let mut codes_and_names = Vec::new();
        let mut new_rule_identifiers = Vec::new();
        match &line.directive {
            Directive::All(_) => {}
            Directive::Codes(directive) => {
                for directive in directive.iter() {
                    let identifier = directive.identifier();

                    match identifier {
                        NoqaIdentifier::Code(original_code) => {
                            let rule_code =
                                get_redirect_target(original_code).unwrap_or(original_code);

                            if Rule::UnusedNOQA.noqa_code() == rule_code {
                                continue 'line;
                            }
                            if let Ok(rule) = Rule::from_code(rule_code) {
                                new_rule_identifiers.push(rule.as_ref().to_string());
                                codes_and_names
                                    .push((rule_code.to_string(), rule.as_ref().to_string()));
                            } else {
                                new_rule_identifiers.push(original_code.to_string());
                            }
                        }
                        NoqaIdentifier::Name(name) => {
                            if Rule::UnusedNOQA.as_ref() == name {
                                continue 'line;
                            }

                            new_rule_identifiers.push(name.to_string());
                        }
                    }
                }
                if !codes_and_names.is_empty() {
                    let mut diagnostic =
                        Diagnostic::new(NOQAByCode { codes_and_names }, directive.range());

                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        format!("# noqa: {}", new_rule_identifiers.join(", ")),
                        directive.range(),
                    )));
                    diagnostics.push(diagnostic);
                }
            }
        }
    }
}
