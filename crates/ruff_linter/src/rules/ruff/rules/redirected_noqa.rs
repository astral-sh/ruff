use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::checkers::ast::LintContext;
use crate::codes::Category;
use crate::noqa::{Codes, Directive, FileNoqaDirectives, NoqaDirectives};
use crate::rule_redirects::get_redirect;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for `noqa` directives that use redirected rule codes.
///
/// In [`preview`] mode, also checks for redirected rule codes and names in
/// `ruff: ignore`, `ruff: file-ignore`, `ruff: disable`, and `ruff: enable` comments.
///
/// ## Why is this bad?
/// When one of Ruff's rule codes or names has been redirected, the implication is that the rule has
/// been deprecated in favor of another rule, code, or name. To keep your codebase
/// consistent and up-to-date, prefer the canonical rule code or name over the deprecated one.
///
/// ## Fix safety
/// The fix is marked unsafe for range suppressions because replacing a redirected
/// code or name can change which `ruff: enable` directive matches a `ruff: disable` directive.
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
#[violation_metadata(stable_since = "0.6.0", category = Category::Suspicious)]
pub(crate) struct RedirectedNOQA {
    pub(crate) original: &'static str,
    pub(crate) target: &'static str,
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
pub(crate) fn redirected_noqa(context: &LintContext, noqa_directives: &NoqaDirectives) {
    for line in noqa_directives.lines() {
        let Directive::Codes(directive) = &line.directive else {
            continue;
        };

        build_diagnostics(context, directive);
    }
}

/// RUF101 for file noqa directives
pub(crate) fn redirected_file_noqa(context: &LintContext, noqa_directives: &FileNoqaDirectives) {
    for line in noqa_directives.lines() {
        let Directive::Codes(codes) = &line.parsed_file_exemption else {
            continue;
        };

        build_diagnostics(context, codes);
    }
}

/// Convert a sequence of [Codes] into [Diagnostic]s and append them to `diagnostics`.
fn build_diagnostics(context: &LintContext, codes: &Codes<'_>) {
    for code in codes.iter() {
        if let Some((original, target)) = get_redirect(code.as_str()) {
            let mut diagnostic =
                context.report_diagnostic(RedirectedNOQA { original, target }, code.range());
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                target.to_string(),
                code.range(),
            )));
        }
    }
}
