use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::noqa::{Directive, NoqaDirectives};
use crate::rule_redirects::get_redirect_target;

/// ## What it does
/// Checks for `noqa` directives with code redirects.
///
/// ## Why is this bad?
/// A code redirect implies that a rule has been deprecated in favor of another rule.
/// To avoid confusion it is better to use the canonical rule code.
///
/// ## Example
/// ```python
/// x = eval(command)  # noqa: PGH001
/// ```
///
/// Use instead:
/// ```python
/// x = eval(command)  # noqa: S307
/// ```
#[violation]
pub struct RedirectedNOQA {
    original: String,
    target: String,
}

impl AlwaysFixableViolation for RedirectedNOQA {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedirectedNOQA { original, target } = self;
        format!("`{original}` is a redirect to `{target}`")
    }

    fn fix_title(&self) -> String {
        let RedirectedNOQA { original, target } = self;
        format!("Replace `{original}` with `{target}`")
    }
}

/// RUF101
pub(crate) fn redirected_noqa(diagnostics: &mut Vec<Diagnostic>, noqa_directives: &NoqaDirectives) {
    for line in noqa_directives.lines() {
        let Directive::Codes(directive) = &line.directive else {
            continue;
        };

        for (original_code, code_range) in directive.codes().iter().zip(directive.code_ranges()) {
            if let Some(redirected) = get_redirect_target(original_code) {
                let mut diagnostic = Diagnostic::new(
                    RedirectedNOQA {
                        original: (*original_code).to_string(),
                        target: redirected.to_string(),
                    },
                    *code_range,
                );

                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    code.to_string(),
                    *code_range,
                )));
                diagnostics.push(diagnostic);
            }
        }
    }
}
