use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::noqa::{Directive, NoqaDirectives, NoqaIdentifier};
use crate::rule_redirects::get_redirect_target;

/// ## What it does
/// Checks for `noqa` directives that use redirected rule codes.
///
/// ## Why is this bad?
/// When a rule code has been redirected, the implication is that the rule has
/// been deprecated in favor of another rule or code. To keep the codebase
/// consistent and up-to-date, prefer the canonical rule code over the deprecated
/// code.
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
        let RedirectedNOQA { target, .. } = self;
        format!("Replace with `{target}`")
    }
}

/// RUF101
pub(crate) fn redirected_noqa(diagnostics: &mut Vec<Diagnostic>, noqa_directives: &NoqaDirectives) {
    for line in noqa_directives.lines() {
        let Directive::Codes(directive) = &line.directive else {
            continue;
        };

        for rule_ident in directive.iter() {
            let NoqaIdentifier::Code(code) = rule_ident.identifier() else {
                continue;
            };

            if let Some(redirected) = get_redirect_target(code) {
                let mut diagnostic = Diagnostic::new(
                    RedirectedNOQA {
                        original: code.to_string(),
                        target: redirected.to_string(),
                    },
                    rule_ident.range(),
                );
                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    redirected.to_string(),
                    rule_ident.range(),
                )));
                diagnostics.push(diagnostic);
            }
        }
    }
}
