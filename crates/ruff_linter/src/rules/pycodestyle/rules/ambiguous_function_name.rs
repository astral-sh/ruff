use ruff_python_ast::Identifier;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::pycodestyle::helpers::is_ambiguous_name;

/// ## What it does
/// Checks for the use of the characters 'l', 'O', or 'I' as function names.
///
/// ## Why is this bad?
/// In some fonts, these characters are indistinguishable from the
/// numerals one and zero. When tempted to use 'l', use 'L' instead.
///
/// ## Example
///
/// ```python
/// def l(x): ...
/// ```
///
/// Use instead:
///
/// ```python
/// def long_name(x): ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.35")]
pub(crate) struct AmbiguousFunctionName(pub String);

impl Violation for AmbiguousFunctionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousFunctionName(name) = self;
        format!("Ambiguous function name: `{name}`")
    }
}

/// E743
pub(crate) fn ambiguous_function_name(checker: &Checker, name: &Identifier) {
    if is_ambiguous_name(name) {
        checker.report_diagnostic(AmbiguousFunctionName(name.to_string()), name.range());
    }
}
