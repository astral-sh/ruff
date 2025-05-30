use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{StringLike, TStringPart};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for implicit concatenation of t-strings with f-strings.
///
/// ## Why is this bad?
/// Template strings (t-strings) are often used to validate or clean
/// certain interpolated expressions before or without evaluating them.
/// Formatted strings (f-strings), by contrast, will evaluate any
/// interpolated expressions eagerly.
///
/// ## Example
/// ```python
/// t"User {inputs}" f" and {unsafe inputs}"
/// ```
///
/// Use instead:
/// ```python
/// t"User {inputs}" t" and {unsafe inputs}"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct ImplicitConcatenationFormatAndTemplateString;

impl Violation for ImplicitConcatenationFormatAndTemplateString {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Implicit concatenation of t-string and f-string".to_string()
    }
}

/// RUF061
pub(crate) fn implicit_concatenation_with_template_string(
    checker: &Checker,
    string_like: StringLike,
) {
    if !checker.target_version().supports_t_strings() {
        return;
    }
    let StringLike::TString(tstring) = string_like else {
        return;
    };

    if tstring.value.iter().any(TStringPart::is_f_string) {
        checker.report_diagnostic(
            ImplicitConcatenationFormatAndTemplateString,
            string_like.range(),
        );
    }
}
