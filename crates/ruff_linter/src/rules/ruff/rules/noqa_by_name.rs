use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::{
    noqa::{Directive, NoqaDirectives, NoqaIdentifier},
    registry::Rule,
    rule_redirects::get_redirect_target,
};

/// ## What it does
/// Checks for uses of rule names in noqa comments.
///
/// ## Why is this bad?
/// Rule names are incompatible with other tools that use rule codes.
///
/// ## Example
/// ```python
/// from typing import Never  # noqa: unused-import
/// ```
///
/// Use instead:
/// ```python
/// from typing import Never  # noqa: F401
/// ```
#[violation]
pub struct NOQAByName {
    pub names_and_codes: Vec<(String, String)>,
}

impl AlwaysFixableViolation for NOQAByName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NOQAByName { names_and_codes } = self;
        format!(
            "Noqa directive lists rule names instead of rule codes: {}",
            names_and_codes
                .iter()
                .map(|(name, code)| format!("{name} ({code})"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    fn fix_title(&self) -> String {
        "Use rule codes in noqa directive".to_string()
    }
}

/// RUF103
pub(crate) fn noqa_by_name(diagnostics: &mut Vec<Diagnostic>, noqa_directives: &NoqaDirectives) {
    'line: for line in noqa_directives.lines() {
        let mut names_and_codes = Vec::new();
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

                            new_rule_identifiers.push(rule_code.to_string());
                        }
                        NoqaIdentifier::Name(name) => {
                            if Rule::UnusedNOQA.as_ref() == name {
                                continue 'line;
                            }

                            if let Ok(rule) = Rule::from_name(name) {
                                new_rule_identifiers.push(rule.noqa_code().to_string());
                                names_and_codes
                                    .push((name.to_string(), rule.noqa_code().to_string()));
                            } else {
                                new_rule_identifiers.push(name.to_string());
                            }
                        }
                    }
                }
                if !names_and_codes.is_empty() {
                    let mut diagnostic =
                        Diagnostic::new(NOQAByName { names_and_codes }, directive.range());

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
