use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::noqa::{Codes, Directive, FileNoqaDirectives, NoqaDirectives};
use crate::rule_redirects::get_redirect_target;

/// ## What it does
/// Checks for `noqa` directives that use redirected rule codes.
///
/// ## Why is this bad?
/// When one of Ruff's rule codes has been redirected, the implication is that the rule has
/// been deprecated in favor of another rule or code. To keep your codebase
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
#[derive(ViolationMetadata)]
pub(crate) struct RedirectedNOQA {
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

/// RUF101 for in-line noqa directives
pub(crate) fn redirected_noqa(diagnostics: &mut Vec<Diagnostic>, noqa_directives: &NoqaDirectives) {
    for line in noqa_directives.lines() {
        let Directive::Codes(directive) = &line.directive else {
            continue;
        };

        build_diagnostics(diagnostics, directive);
    }
}

/// RUF101 for file noqa directives
pub(crate) fn redirected_file_noqa(
    diagnostics: &mut Vec<Diagnostic>,
    noqa_directives: &FileNoqaDirectives,
) {
    for line in noqa_directives.lines() {
        let Directive::Codes(codes) = &line.parsed_file_exemption else {
            continue;
        };

        build_diagnostics(diagnostics, codes);
    }
}

/// Convert a sequence of [Codes] into [Diagnostic]s and append them to `diagnostics`.
fn build_diagnostics(diagnostics: &mut Vec<Diagnostic>, codes: &Codes<'_>) {
    for code in codes.iter() {
        if let Some(redirected) = get_redirect_target(code.as_str()) {
            let mut diagnostic = Diagnostic::new(
                RedirectedNOQA {
                    original: code.to_string(),
                    target: redirected.to_string(),
                },
                code.range(),
            );
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                redirected.to_string(),
                code.range(),
            )));
            diagnostics.push(diagnostic);
        }
    }
}
